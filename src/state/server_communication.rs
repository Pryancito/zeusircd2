use lapin::{
    options::*,
    types::FieldTable,
    Connection, ConnectionProperties,
    BasicProperties,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::result::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::*;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicUsize};
use futures::stream::StreamExt;
use tracing::{error, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerMessage {
    Connect {
        server_name: String,
        host: String,
        port: u16,
    },
    Disconnect {
        server_name: String,
    },
    Broadcast {
        from_server: String,
        message: String,
    },
    ServerLink {
        server_name: String,
        host: String,
        port: u16,
        hop_count: u32,
        description: String,
    },
    ServerUnlink {
        server_name: String,
    },
}

#[derive(Clone)]
pub struct ServerCommunication {
    amqp_url: String,
    exchange: String,
    queue: String,
    main_state: Option<Arc<Mutex<MainState>>>,
    server_name: String,
    connected: bool,
    connection: Arc<Mutex<Option<Connection>>>,  // Envolvemos en Arc<Mutex> para permitir Clone
    channel: Option<lapin::Channel>,                    // Canal actual
}

impl ServerCommunication {
    pub async fn new(amqp_url: &str, server: &String, exchange: &str, queue: &str) -> Self {
        let server_comm = Self {
            amqp_url: amqp_url.to_string(),
            exchange: exchange.to_string(),
            queue: queue.to_string(),
            main_state: None,
            server_name: server.to_string(),
            connected: false,
            connection: Arc::new(Mutex::new(None)),  // Inicializado como None dentro de Arc<Mutex>
            channel: None,
        };
        server_comm
    }

    pub fn set_main_state(&mut self, main_state: Arc<Mutex<MainState>>) {
        self.main_state = Some(main_state);
    }

    fn get_main_state(&self) -> Option<Arc<Mutex<MainState>>> {
        self.main_state.clone()
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn Error>> {
        if self.connected {
            return Err("Already connected to this server".into());
        }
        // Crear la conexión AMQP
        let connection = Connection::connect(&self.amqp_url, ConnectionProperties::default()).await?;
        let channel = connection.create_channel().await?;

        // Guardar la conexión
        let mut conn = self.connection.lock().await;
        *conn = Some(connection);
        self.connected = true;

        // Declarar el exchange
        channel
            .exchange_declare(
                &self.exchange,
                lapin::ExchangeKind::Fanout,
                ExchangeDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        // Declarar la cola
        channel
            .queue_declare(
                &self.queue,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        // Vincular la cola al exchange
        channel
            .queue_bind(
                &self.queue,
                &self.exchange,
                &self.server_name.to_string(),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;

        self.channel = Some(channel);
        info!("Connected to AMQP.");

        Ok(())
    }

    pub async fn disconnect_server(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.connected {
            return Err("Not connected to this server".into());
        }

        // Cerrar la conexión AMQP
        let mut conn = self.connection.lock().await;
        if let Some(connection) = conn.take() {
            connection.close(200, "Cierre normal").await?;
            self.connected = false;
        }
        info!("Disconnected from AMQP.");

        Ok(())
    }

    pub async fn publish_message(&self, message: &String) -> Result<(), Box<dyn Error>> {
        // Serializar el mensaje a JSON
        let message_bytes = serde_json::to_vec(&message)?;
        // Publicar el mensaje en el exchange
        self.channel.as_ref().unwrap()
            .basic_publish(
                &self.exchange,
                &self.server_name,
                BasicPublishOptions::default(),
                &message_bytes,
                BasicProperties::default(),
            )
            .await?;

        Ok(())
    }

    pub async fn consume_messages<F>(&self, callback: F) -> Result<(), Box<dyn Error + Send + Sync>>
    where
        F: Fn(ServerMessage) -> Result<(), Box<dyn Error>> + Send + Sync + 'static,
    {
        // Configurar el consumidor
        let mut consumer = self.channel.as_ref().unwrap()
            .basic_consume(
                &self.queue,
                "zeusircd2_consumer",
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .expect("Failed to start AMQP consumer");

        // Procesar mensajes entrantes
        // Bucle para recibir mensajes
        while let Some(delivery) = consumer.next().await {
            match delivery {
                Ok(delivery) => {
                    // Deserializar el mensaje
                    if let Ok(message) = serde_json::from_slice::<ServerMessage>(&delivery.data) {
                        // Ejecutar el callback con el mensaje
                        match callback(message.clone()).map_err(|e| e.to_string()) {
                            Ok(_) => {
                                let msg = format!("{:?}", message);
                                self.server_message(msg).await;
                                warn!("{:?}", message);
                            },
                            Err(e) => {
                                error!("Error procesando mensaje: {:?}", e);
                            }
                        }
                    }

                    // Envía el ACK al servidor AMQP
                    let _ = delivery
                        .ack(lapin::options::BasicAckOptions::default())
                        .await;
                }
                Err(e) => {
                    error!("Error recibiendo mensaje de AMQP: {:?}", e);
                }
            }
        }

        Ok(())
    }

    async fn server_message(&self, message: String) {
        // Manejo de errores más robusto
        let result = match self.parse_server_message(message.clone()) {
            Ok(r) => r,
            Err(e) => {
                error!("Error al parsear mensaje del servidor: {}", e);
                return;
            }
        };

        if !self.connected {
            error!("Not connected server.");
            return;
        }

        let stream = None;

        let ip_addr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let conns_count = Arc::new(AtomicUsize::new(0));
        let mut conn_state = ConnState::new(ip_addr, stream.expect("Remote user connection"), conns_count);

        // Procesar comandos de manera más estructurada
        match result.get_command() {
            "PRIVMSG" => {
                if let Some(main_state) = self.get_main_state() {
                    let state = main_state.lock().await;
                    let parts: Vec<&str> = result.get_text().splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let targets: Vec<&str> = parts[0].split_whitespace().collect();
                        let text = parts[1];
                        let _ = state.process_privmsg(&mut conn_state, targets, text).await;
                    }
                }
            }
            "NOTICE" => {
                if let Some(main_state) = self.get_main_state() {
                    let state = main_state.lock().await;
                    let parts: Vec<&str> = result.get_text().splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let targets: Vec<&str> = parts[0].split_whitespace().collect();
                        let text = parts[1];
                        let _ = state.process_notice(&mut conn_state, targets, text).await;
                    }
                }
            }
            "MODE" => {
                if let Some(main_state) = self.get_main_state() {
                    let state = main_state.lock().await;
                    let parts: Vec<&str> = result.get_text().split_whitespace().collect();
                    if parts.len() >= 3 {
                        let channel = parts[0];
                        let mode = parts[1];
                        let mask = parts[2];
                        
                        // Procesar modo de ban (+b o -b)
                        if mode == "+b" || mode == "-b" {
                            let _ = state.process_mode(&mut conn_state, channel, vec![(mode, vec![mask])]).await;
                        }
                    }
                }
            }
            _ => {
                error!("Server Message error: Comando desconocido {}", result.get_command());
            }
        }
    }

    fn parse_server_message(&self, message: String) -> Result<ServMessage, Box<dyn Error>> {
        let message = message.trim();
        
        // Verificar que el mensaje comienza con ':'
        if !message.starts_with(':') {
            return Err("Mensaje inválido: debe comenzar con ':'".into());
        }

        // Extraer el servidor
        let server_end = message.find(' ').ok_or("Formato de mensaje inválido")?;
        let server = message[1..server_end].to_string();
        
        // Extraer el resto del mensaje
        let remaining = message[server_end + 1..].trim();
        
        // Extraer nick!id@host
        let user_end = remaining.find(' ').ok_or("Formato de mensaje inválido")?;
        let user = remaining[..user_end].to_string();
        
        // Extraer el comando y el texto
        let command_text = remaining[user_end + 1..].trim();
        let command_end = command_text.find(':').unwrap_or(command_text.len());
        let command = command_text[..command_end].trim().to_string();
        
        // Extraer el texto (si existe)
        let text = if command_end < command_text.len() {
            command_text[command_end + 1..].trim().to_string()
        } else {
            String::new()
        };

        Ok(ServMessage {
            server,
            user,
            command,
            text
        })
    }

    fn parse_user(user: String) -> Result<ServerUser, Box<dyn Error>> {
        // Verificar que el formato es correcto (nick!ident@host)
        if !user.contains('!') || !user.contains('@') {
            return Err("Formato de usuario inválido: debe ser nick!ident@host".into());
        }

        // Extraer nick
        let nick_end = user.find('!').ok_or("Formato de usuario inválido")?;
        let nick = user[..nick_end].to_string();

        // Extraer ident
        let ident_start = nick_end + 1;
        let ident_end = user.find('@').ok_or("Formato de usuario inválido")?;
        let ident = user[ident_start..ident_end].to_string();

        // Extraer host
        let host = user[ident_end + 1..].to_string();

        Ok(ServerUser {
            nick,
            ident,
            host
        })
    }
}

#[derive(Clone)]
pub struct ServMessage {
    server: String,
    user: String,
    command: String,
    text: String,
}

#[derive(Clone)]
pub struct ServerUser {
    nick: String,
    ident: String,
    host: String,
}

impl ServMessage {
    pub fn get_command(&self) -> &str {
        &self.command
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }

    pub fn get_server(&self) -> &str {
        &self.server
    }

    pub fn get_user(&self) -> &str {
        &self.user
    }
} 