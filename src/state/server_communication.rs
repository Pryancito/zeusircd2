use lapin::{
    options::*,
    types::FieldTable,
    Connection, ConnectionProperties,
    BasicProperties,
};
use std::error::Error;
use std::result::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::*;
use futures::stream::StreamExt;
use tracing::error;
use std::time::{UNIX_EPOCH, SystemTime};
use tokio::time::Duration;

#[derive(Clone)]
pub(crate) struct ServerCommunication {
    amqp_url: String,
    exchange: String,
    queue: String,
    server_name: String,
    connected: bool,
    connection: Arc<Mutex<Option<Connection>>>,
    channel: Arc<Mutex<Option<lapin::Channel>>>,
    pub(super) state: Arc<RwLock<VolatileState>>,
}

impl Drop for ServerCommunication {
    fn drop(&mut self) {
        let _ = self.disconnect_server();
    }
}

impl ServerCommunication {
    pub(crate) async fn new(state: &Arc<RwLock<VolatileState>>, amqp_url: &str, server: &String, exchange: &str, queue: &str) -> Self {
        let server_comm = Self {
            amqp_url: amqp_url.to_string(),
            exchange: exchange.to_string(),
            queue: queue.to_string(),
            server_name: server.to_string(),
            connected: false,
            connection: Arc::new(Mutex::new(None)),
            channel: Arc::new(Mutex::new(None)),
            state: state.clone(),
        };
        server_comm
    }

    pub(crate) async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.connected {
            return Err("Already connected to this server".into());
        }
        let connection = Connection::connect(&self.amqp_url, ConnectionProperties::default()).await?;
        let channel = connection.create_channel().await?;

        let mut conn = self.connection.lock().await;
        *conn = Some(connection);
        
        let mut chan = self.channel.lock().await;
        *chan = Some(channel.clone());
        
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
                "",  // routing key vacía para fanout
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;
        info!("Connected to AMQP. Channel: {:?}", channel.id());
        Ok(())
    }

    pub(crate) async fn disconnect_server(&mut self) -> Result<(), Box<dyn Error>> {
        if !self.connected {
            return Err("Not connected to this server".into());
        }

        // Cerrar la conexión AMQP
        let mut conn = self.connection.lock().await;
        if let Some(connection) = conn.take() {
            connection.close(200, "Server shutdown").await?;
            self.connected = false;
        }
        info!("Disconnected from AMQP.");
        Ok(())
    }

    pub async fn monitor_amqp_connections(
        amqp_url: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn = Connection::connect(amqp_url, ConnectionProperties::default()).await?;
        let channel = conn.create_channel().await?;
    
        // 1. Declarar una cola para los eventos
        let queue_name = "server_connection_events";
        let _queue = channel.queue_declare(
            queue_name,
            QueueDeclareOptions {
                durable: false,      // No persistir la cola
                exclusive: true,     // Solo este consumidor puede acceder
                auto_delete: true,   // Eliminar cuando el consumidor se desconecte
                ..Default::default()
            },
            FieldTable::default(),
        ).await?;
    
        // 2. Vincular la cola al exchange de eventos de RabbitMQ
        // Para capturar todos los eventos de conexión/desconexión
        channel.queue_bind(
            queue_name,
            "amq.rabbitmq.event", // O "amq.rabbitmq.log"
            "connection.#",       // Binding key para eventos de conexión
            QueueBindOptions::default(),
            FieldTable::default(),
        ).await?;
    
        info!("Escuchando eventos de conexión/desconexión AMQP en la cola '{}'", queue_name);
    
        // 3. Iniciar el consumidor
        let mut consumer = channel.basic_consume(
            queue_name,
            "connection_monitor_consumer",
            BasicConsumeOptions::default(),
            FieldTable::default(),
        ).await?;
    
        while let Some(delivery) = consumer.next().await {
            let delivery = delivery?;
            let routing_key = delivery.routing_key.as_str();
            let payload_str = String::from_utf8_lossy(&delivery.data);
    
            info!("Evento AMQP recibido: Routing Key='{}'", routing_key);
    
            // Intenta parsear el payload como JSON (si usas amq.rabbitmq.event)
            match serde_json::from_str::<serde_json::Value>(&payload_str) {
                Ok(json_event) => {
                    info!("  Payload JSON: {:?}", json_event);
                    // Aquí puedes extraer información como connection_name, peer_address, etc.
                    if routing_key == "connection.created" {
                        info!("    --> ¡Nueva conexión detectada! Cliente: {:?}", json_event["connection_name"]);
                    } else if routing_key == "connection.closed" {
                        info!("    --> ¡Conexión cerrada detectada! Cliente: {:?}", json_event["connection_name"]);
                        info!("    Razón: {:?}", json_event["reason"]);
                    }
                    // ... y actualizar tu estado interno de la red IRC
                }
                Err(_) => {
                    // Si no es JSON o si usas amq.rabbitmq.log
                    info!("  Payload (texto): {}", payload_str);
                }
            }
            delivery.ack(Default::default()).await?;
        }
    
        Ok(())
    }

    pub(crate) async fn publish_message(&self, message: &String) -> Result<(), Box<dyn Error>> {
        // Serializar el mensaje a JSON
        let message_bytes = serde_json::to_vec(&message)?;
        // Publicar el mensaje en el exchange
        self.channel.lock().await.as_ref().unwrap()
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

    pub(crate) async fn consume_messages<F, Fut>(&self, callback: F) -> Result<(), Box<dyn Error + Send + Sync>>
    where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send + 'static,
    {
        let channel_guard = self.channel.lock().await;
        let channel = channel_guard.as_ref().ok_or("No hay canal AMQP disponible")?;
        
        let mut consumer = channel.basic_consume(
            &self.queue,
            "",
            BasicConsumeOptions {
                no_ack: false,
                ..Default::default()
            },
            FieldTable::default(),
        ).await?;


        let server_comm = self.clone();
        tokio::spawn(async move {
            while let Some(delivery) = consumer.next().await {
                match delivery {
                    Ok(delivery) => {
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
        let amqp_url = self.amqp_url.clone();
        tokio::spawn(async move {
            ServerCommunication::monitor_amqp_connections(&amqp_url).await.unwrap();
            info!("Monitor de conexiones AMQP finalizado");
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });
        Ok(())
    }

    pub(crate) async fn server_message(&self, message: String) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Manejo de errores más robusto
        let result = match self.parse_server_message(message.clone()) {
            Ok(r) => {
                r
            },
            Err(e) => {
                error!("Error al parsear mensaje del servidor: {}", e);
                return Err(e);
            }
        };
        let command = result.get_command().split_whitespace().nth(0).unwrap_or("");
        // Procesar comandos de manera más estructurada
        match command {
            "PRIVMSG" | "NOTICE" => {
                let channel = result.get_command().split_whitespace().nth(1).unwrap_or("");
                let text = result.get_text();
                let source = self.parse_user(result.get_user().to_string());
                let snick = source.unwrap().nick.clone();
                // Crear un mensaje que simule venir del servidor
                let server_message = format!("{command} {channel} :{text}");
                let state = self.state.read().await;
                // Verificar si el canal existe
                if let Some(chanobj) = state.channels.get(&crate::state::structs::to_unicase(channel)) {
                    let nicks: Vec<String> = chanobj.users.keys().map(|k| k.to_string()).collect();
                    for nick in nicks {
                        if *nick != snick {
                            if let Some(user) = state.users.get(&crate::state::structs::to_unicase(&nick)) {
                                if user.server != result.get_server() {
                                    let _ = user.send_msg_display(
                                        result.get_user(),
                                        server_message.as_str()
                                    );
                                }
                            } else {
                                error!("Error enviando mensaje a usuario {}", nick);
                            }
                        }
                    }
                } else {
                    error!("Canal {} no encontrado", channel);
                }
            }
            "MODE" => {
                let parts: Vec<&str> = message.split_whitespace().collect();
                let channel = parts[3];
                let mode = parts[4];
                let mask = parts[5];

                let server_message = format!("{command} {channel} {mode} {mask}");

                // Procesar modo de ban (+b o -b)
                if mode == "+B" {
                    let mut state = self.state.write().await;
                    let source = self.parse_user(result.get_user().to_string());
                    let snick = source.unwrap().nick.clone();
                    let chanobj: &mut Channel = state.channels.get_mut(&crate::state::structs::to_unicase(channel)).ok_or("Canal no encontrado")?;
                    let mut gban = chanobj.modes.global_ban.take().unwrap_or_default();
                    let norm_bmask = normalize_sourcemask(mask);
                    gban.insert(norm_bmask.clone());
                    let (_, duration) = if let Some(idx) = mask.find('|') {
                        let (mask_part, duration_part) = mask.split_at(idx);
                        let duration = duration_part[1..].parse::<u64>().ok();
                        (mask_part.to_string(), duration)
                    } else {
                        (mask.to_string(), None)
                    };
                    chanobj.ban_info.insert(
                        crate::state::structs::to_unicase(&norm_bmask),
                        BanInfo {
                            who: snick.to_string(),
                            set_time: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                            expires_at: duration,
                        },
                    );
                    chanobj.modes.global_ban = Some(gban);
                    let nicks: Vec<String> = chanobj.users.keys().map(|k| k.to_string()).collect();
                    for nick in nicks {
                        if let Some(user) = state.users.get_mut(&crate::state::structs::to_unicase(&nick.to_string())) {
                            if user.server != result.get_server() {
                                let _ = user.send_msg_display(
                                    result.get_user(),
                                    server_message.as_str()
                                );
                            }
                        } else {
                            error!("Error poniendo ban global en el canal {}", channel);
                        }
                    }
                    if let Some(duration) = duration {
                        let channel_name = channel.to_string();
                        let ban_mask_for_timeout = norm_bmask.clone();
                        let state_clone = self.state.clone();

                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(duration)).await;

                            // Remover el ban global expirado
                            let mut state = state_clone.write().await;
                            if let Some(channel) = state.channels.get_mut(&crate::state::structs::to_unicase(&channel_name)) {
                                if let Some(ban_set) = &mut channel.modes.global_ban {
                                    ban_set.remove(&ban_mask_for_timeout);
                                    channel.ban_info.remove(&crate::state::structs::to_unicase(&ban_mask_for_timeout));

                                    // Notificar a los usuarios del canal
                                    let nicks: Vec<String> = channel.users.keys().map(|k| k.to_string()).collect();
                                    for nick in nicks {
                                        if let Some(user) = state.users.get_mut(&crate::state::structs::to_unicase(&nick)) {
                                            if user.server != result.get_server() {
                                                let _ = user.send_msg_display(
                                                    result.get_user(),
                                                    server_message.as_str()
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        });
                    }
                } else if mode == "-B" {
                    let mut state = self.state.write().await;
                    let chanobj: &mut Channel = state.channels.get_mut(&crate::state::structs::to_unicase(channel)).ok_or("Canal no encontrado")?;
                    let mut gban = chanobj.modes.global_ban.take().unwrap_or_default();
                    let norm_bmask = normalize_sourcemask(mask);
                    gban.remove(&norm_bmask);
                    chanobj.ban_info.remove(&crate::state::structs::to_unicase(&norm_bmask));
                    let nicks: Vec<String> = chanobj.users.keys().map(|k| k.to_string()).collect();
                    for nick in nicks {
                        if let Some(user) = state.users.get_mut(&crate::state::structs::to_unicase(&nick)) {
                            if user.server != result.get_server() {
                                let _ = user.send_msg_display(
                                    result.get_user(),
                                    server_message.as_str()
                                );
                            }
                        } else {
                            error!("Error quitando ban global en el canal {}", channel);
                        }
                    }
                }
            }
            _ => {
                error!("Server Message error: Comando desconocido {}", result.get_command());
            }
        }
        Ok(())
    }

    pub(crate) fn parse_server_message(&self, message: String) -> Result<ServMessage, Box<dyn Error + Send + Sync>> {
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

    pub(crate) fn parse_user(&self, user: String) -> Result<ServerUser, Box<dyn Error>> {
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

    pub(crate) async fn start_consuming(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if !self.connected {
            return Err("No hay conexión AMQP activa".into());
        }

        self.consume_messages(move |_message| {
            async move {
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

#[allow(dead_code)]
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