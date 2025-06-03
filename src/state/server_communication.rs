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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::Duration;

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

        println!("Configurando exchange: {}", self.exchange);
        // Declarar el exchange
        channel
            .exchange_declare(
                &self.exchange,
                lapin::ExchangeKind::Fanout,
                ExchangeDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        println!("Configurando cola: {}", self.queue);
        // Declarar la cola
        channel
            .queue_declare(
                &self.queue,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        println!("Vinculando cola {} al exchange {}", self.queue, self.exchange);
        // Vincular la cola al exchange
        channel
            .queue_bind(
                &self.queue,
                &self.exchange,
                "",  // routing key vacía para fanout
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;

        self.channel = Some(channel);
        
        info!("Connected to AMQP. Channel: {:?}", self.channel.as_ref().unwrap().id());
        self.connected = true;

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

    pub async fn consume_messages<F, Fut>(&self, callback: F) -> Result<(), Box<dyn Error + Send + Sync>>
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send + 'static,
    {
        println!("Iniciando consumo de mensajes...");
        
        let channel = self.channel.as_ref().ok_or("No hay canal AMQP disponible")?;
        println!("Canal AMQP encontrado, configurando consumidor");
        
        let mut consumer = channel.basic_consume(
            &self.queue,
            &"",
            BasicConsumeOptions {
                no_ack: false,
                ..Default::default()
            },
            FieldTable::default(),
        ).await?;

        println!("Consumidor configurado, esperando mensajes...");

        let server_comm = self.clone();
        tokio::spawn(async move {
            while let Some(delivery) = consumer.next().await {
                match delivery {
                    Ok(delivery) => {
                        println!("Mensaje AMQP preparado");
                        if let Ok(message) = serde_json::from_slice::<String>(&delivery.data) {
                            let msg = message.clone();
                            match callback(message.clone()).await {
                                Ok(_) => {
                                    let _ = server_comm.server_message(msg).await;
                                },
                                Err(e) => {
                                    error!("Error procesando mensaje AMQP: {:?}", e);
                                }
                            }
                        }
                        if let Err(e) = delivery.ack(BasicAckOptions::default()).await {
                            error!("Error confirmando mensaje AMQP: {:?}", e);
                        }
                    }
                    Err(e) => {
                        error!("Error recibiendo mensaje AMQP: {:?}", e);
                    }
                }
            }
        });
        Ok(())
    }

    pub async fn server_message(&self, message: String) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!("Server message: {}", message);
        // Manejo de errores más robusto
        let result = match self.parse_server_message(message.clone()) {
            Ok(r) => r,
            Err(e) => {
                error!("Error al parsear mensaje del servidor: {}", e);
                return Err(e);
            }
        };

        if !self.connected {
            return Err("Not connected server.".into());
        }

        if let Some(main_state) = self.get_main_state() {
            let state = main_state.lock().await;
            println!("Procesando mensaje: {}", result.get_text());
            // Procesar comandos de manera más estructurada
            match result.get_command() {
                "PRIVMSG"|"NOTICE" => {
                    let parts: Vec<&str> = result.get_text().splitn(2, ':').collect();
                    if parts.len() == 2 {
                        let targets: Vec<&str> = parts[0].split_whitespace().collect();
                        let text = parts[1];
                        
                        // Obtener el canal del mensaje
                        if let Some(channel) = targets.first() {
                            // Obtener todos los usuarios en el canal
                            if let Some(channel_users) = state.state.read().await.channels.get(*channel) {
                                // Enviar el mensaje a cada usuario en el canal
                                for (nick, _) in &channel_users.users {
                                    if let Some(user) = state.state.read().await.users.get(nick) {
                                        let msg = format!(":{} {} {} :{}", 
                                            result.get_user(),
                                            result.get_command(),
                                            channel,
                                            result.get_text());
                                        let _ = user.send_msg_display(result.get_server(), msg);
                                    }
                                }
                            }
                        }
                    }
                }
                /*"MODE" => {
                    let parts: Vec<&str> = result.get_text().split_whitespace().collect();
                    if parts.len() >= 3 {
                        let channel = parts[0];
                        let mode = parts[1];
                        let mask = parts[2];
                        
                        // Procesar modo de ban (+b o -b)
                        if mode == "+b" || mode == "-b" {
                            // Obtener todas las conexiones activas
                            for conn in state.get_connections() {
                                if let Ok(mut conn_state) = conn.lock() {
                                    let _ = state.process_mode(&mut conn_state, channel, vec![(mode, vec![mask])]).await;
                                }
                            }
                        }
                    }
                }*/
                _ => {
                    error!("Server Message error: Comando desconocido {}", result.get_command());
                }
            }
        }
        Ok(())
    }

    fn parse_server_message(&self, message: String) -> Result<ServMessage, Box<dyn Error + Send + Sync>> {
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

    pub async fn start_consuming(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if !self.connected {
            return Err("No hay conexión AMQP activa".into());
        }

        let server_comm = self.clone();
        self.consume_messages(move |message| {
            let server_comm = server_comm.clone();
            println!("Consumiendo mensaje: {:?}", message);
            async move {
                // Deserializar el mensaje JSON a ServerMessage
                if let Ok(server_message) = serde_json::from_str::<ServerMessage>(&message) {
                    match server_message {
                        ServerMessage::Connect { server_name, host, port } => {
                            println!("Recibido mensaje de conexión del servidor {} ({})", server_name, host);
                        },
                        ServerMessage::Disconnect { server_name } => {
                            println!("Recibido mensaje de desconexión del servidor {}", server_name);
                        },
                        ServerMessage::Broadcast { from_server, message } => {
                            println!("Recibido broadcast del servidor {}: {}", from_server, message);
                        },
                        ServerMessage::ServerLink { server_name, host, port, hop_count, description } => {
                            println!("Recibido link del servidor {} ({})", server_name, host);
                        },
                        ServerMessage::ServerUnlink { server_name } => {
                            println!("Recibido unlink del servidor {}", server_name);
                        },
                    }
                } else {
                    error!("Error al deserializar mensaje: {}", message);
                }
                Ok(())
            }
        }).await
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

#[tokio::test]
async fn test_server_message_handling() {
    // Configuración del servidor
    let config = MainConfig {
        // Configura los valores necesarios
        ..Default::default()
    };

    // Inicia el servidor
    let (server, _) = run_server(config).await.unwrap();

    // Conecta un cliente de prueba
    let mut stream = TcpStream::connect("127.0.0.1:6667").await.unwrap();
    
    // Envía un mensaje de prueba
    let test_message = "PRIVMSG #test :Hola mundo\r\n";
    stream.write_all(test_message.as_bytes()).await.unwrap();

    // Lee la respuesta
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await.unwrap();
    let response = String::from_utf8_lossy(&buffer[..n]);

    // Verifica la respuesta
    assert!(response.contains("Hola mundo"));
}

#[tokio::test]
async fn test_server_multiple_messages() {
    let config = MainConfig {
        // Configura los valores necesarios
        ..Default::default()
    };

    let (server, _) = run_server(config).await.unwrap();
    let mut stream = TcpStream::connect("127.0.0.1:6667").await.unwrap();

    // Envía múltiples mensajes
    let messages = vec![
        "PRIVMSG #test :Mensaje 1\r\n",
        "PRIVMSG #test :Mensaje 2\r\n",
        "PRIVMSG #test :Mensaje 3\r\n",
    ];

    for msg in messages {
        stream.write_all(msg.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Verifica las respuestas
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await.unwrap();
    let response = String::from_utf8_lossy(&buffer[..n]);

    assert!(response.contains("Mensaje 1"));
    assert!(response.contains("Mensaje 2"));
    assert!(response.contains("Mensaje 3"));
}

#[tokio::test]
async fn test_server_invalid_message() {
    let config = MainConfig {
        // Configura los valores necesarios
        ..Default::default()
    };

    let (server, _) = run_server(config).await.unwrap();
    let mut stream = TcpStream::connect("127.0.0.1:6667").await.unwrap();

    // Envía un mensaje inválido
    let invalid_message = "INVALID_COMMAND\r\n";
    stream.write_all(invalid_message.as_bytes()).await.unwrap();

    // Verifica que el servidor maneja el error correctamente
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await.unwrap();
    let response = String::from_utf8_lossy(&buffer[..n]);

    assert!(response.contains("ERROR"));
}

#[tokio::test]
async fn test_server_message_encoding() {
    let config = MainConfig {
        // Configura los valores necesarios
        ..Default::default()
    };

    let (server, _) = run_server(config).await.unwrap();
    let mut stream = TcpStream::connect("127.0.0.1:6667").await.unwrap();

    // Envía un mensaje con caracteres especiales
    let special_message = "PRIVMSG #test :¡Hola! ¿Cómo estás? 你好\r\n";
    stream.write_all(special_message.as_bytes()).await.unwrap();

    // Verifica que el servidor maneja correctamente la codificación
    let mut buffer = [0; 1024];
    let n = stream.read(&mut buffer).await.unwrap();
    let response = String::from_utf8_lossy(&buffer[..n]);

    assert!(response.contains("¡Hola! ¿Cómo estás? 你好"));
}