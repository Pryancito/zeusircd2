use lapin::{
    options::*,
    types::FieldTable,
    Connection, ConnectionProperties,
    BasicProperties,
};
use lapin::types::AMQPValue;
use std::error::Error;
use std::result::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::*;
use futures::stream::StreamExt;
use tracing::{error, info};
use std::time::{UNIX_EPOCH, SystemTime};
use tokio::time::Duration;
use uuid::Uuid;
use serde_json;
use std::collections::HashMap;

#[derive(Clone)]
pub(crate) struct ServerInfo {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) uuid: Uuid,
}

#[derive(Clone)]
pub(crate) struct ServerCommunication {
    amqp_url: String,
    exchange: String,
    queue: String,
    server_name: String,
    uuid: Uuid,
    connected: bool,
    connection: Arc<Mutex<Option<Connection>>>,
    channel: Arc<Mutex<Option<lapin::Channel>>>,
    conn_channel: Arc<Mutex<Option<lapin::Channel>>>,
    conn_queue: String,
    pub(super) state: Arc<RwLock<VolatileState>>,
    pub(super) servers: Arc<RwLock<HashMap<String, ServerInfo>>>,
}

impl Drop for ServerCommunication {
    fn drop(&mut self) {
        let _ = self.disconnect_server();
    }
}

impl ServerCommunication {
    pub(crate) async fn new(state: &Arc<RwLock<VolatileState>>, amqp_url: &str, server: &String, exchange: &str, queue: &str) -> Self {
        // Generar el UUID primero para poder usarlo en el mapa y en la estructura
        let generated_uuid = Uuid::new_v4();
        let server_comm = Self {
            amqp_url: amqp_url.to_string(),
            exchange: exchange.to_string(),
            queue: queue.to_string(),
            server_name: server.to_string(),
            uuid: generated_uuid,
            connected: false,
            connection: Arc::new(Mutex::new(None)),
            channel: Arc::new(Mutex::new(None)),
            conn_channel: Arc::new(Mutex::new(None)),
            conn_queue: format!("connection_events_{queue}"),
            state: state.clone(),
            // Insertar el propio servidor en el mapa de servidores
            servers: Arc::new(RwLock::new({
                let mut map = HashMap::new();
                map.insert(
                    generated_uuid.to_string(),
                    ServerInfo {
                        name: server.to_string(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        uuid: generated_uuid,
                    },
                );
                map
            })),
        };
        server_comm
    }

    pub(crate) async fn connect(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        if self.connected {
            return Err("Already connected to this server".into());
        }

        let mut connection_properties = ConnectionProperties::default();
        connection_properties.client_properties.insert("irc_server_name".into(), AMQPValue::LongString(self.server_name.clone().into()));
        connection_properties.client_properties.insert("irc_server_version".into(), AMQPValue::LongString(env!("CARGO_PKG_VERSION").into()));
        connection_properties.client_properties.insert("irc_server_uuid".into(), AMQPValue::LongString(self.uuid.to_string().into()));

        let connection = Connection::connect(&self.amqp_url, connection_properties).await?;
        self.connection = Arc::new(Mutex::new(Some(connection)));
        let channel = self.connection.lock().await.as_ref().unwrap().create_channel().await?;
        self.channel = Arc::new(Mutex::new(Some(channel)));
        self.connected = true;

        // Declarar el exchange
        self.channel.lock().await.as_ref().unwrap()
            .exchange_declare(
                &self.exchange,
                lapin::ExchangeKind::Fanout,
                ExchangeDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        // Declarar la cola
        self.channel.lock().await.as_ref().unwrap()
            .queue_declare(
                &self.queue,
                QueueDeclareOptions::default(),
                FieldTable::default(),
            )
            .await?;

        // Vincular la cola al exchange
        self.channel.lock().await.as_ref().unwrap()
            .queue_bind(
                &self.queue,
                &self.exchange,
                "",  // routing key vacía para fanout
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;
        info!("Connected to AMQP. Channel: {:?}", self.channel.lock().await.as_ref().unwrap().id());

        let conn_channel = self.connection.lock().await.as_ref().unwrap().create_channel().await?;
        self.conn_channel = Arc::new(Mutex::new(Some(conn_channel)));

        self.conn_channel.lock().await.as_ref().unwrap()
            .queue_declare(
            &self.conn_queue,
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
        self.channel.lock().await.as_ref().unwrap()
            .queue_bind(
            &self.conn_queue,
            "amq.rabbitmq.event", // O "amq.rabbitmq.log"
            "connection.#",       // Binding key para eventos de conexión
            QueueBindOptions::default(),
            FieldTable::default(),
        ).await?;

        info!("Connected to AMQP Connection listener. Channel: {:?}", self.channel.lock().await.as_ref().unwrap().id());
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

    pub(crate) async fn publish_message(&self, message: &String) -> Result<(), Box<dyn Error>> {
        // Serializar el mensaje a JSON
        let message = format!(":{} {}", self.uuid, message);
        let message_bytes = serde_json::to_vec(&message)?;
        // Publicar el mensaje en el exchange
        self.channel.lock().await.as_ref().unwrap()
            .basic_publish(
                &self.exchange,
                "",
                BasicPublishOptions::default(),
                &message_bytes,
                BasicProperties::default(),
            )
            .await?;

        Ok(())
    }

    pub(crate) async fn consume_messages<F, Fut>(&self, callback: F) -> Result<(), Box<dyn Error + Send + Sync>>
    where
        F: Fn(String) -> Fut + Send + Sync + Clone + 'static,
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
        let callback_clone = callback.clone();
        tokio::spawn(async move {
            while let Some(delivery_result) = consumer.next().await {
                match delivery_result {
                    Ok(delivery) => {
                        if let Ok(message) = serde_json::from_slice::<String>(&delivery.data) {
                            let msg = message.clone();
                            match callback_clone(message.clone()).await {
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

        // Iniciar el monitor de conexiones en un hilo separado
        let mut server_comm_monitor = self.clone();
        tokio::spawn(async move {
            let _ = server_comm_monitor.monitor_amqp_connections().await;
            info!("Monitor de conexiones AMQP finalizado");
        });

        Ok(())
    }

    pub(crate) async fn monitor_amqp_connections(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let conn_channel = self.conn_channel.lock().await;
        let channel = conn_channel.as_ref().ok_or("No hay canal AMQP disponible")?;
        let mut consumer = channel.basic_consume(
            &self.conn_queue,
            &self.uuid.to_string(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        ).await?;
    
        while let Some(delivery) = consumer.next().await {
            let delivery = delivery?;
            let routing_key = delivery.routing_key.as_str();
    
            // Intenta parsear el payload como JSON (si usas amq.rabbitmq.event)
            if let Some(headers) = delivery.properties.headers() {
                if let Some(client_props_value) = headers.inner().get("client_properties") {
                    if let AMQPValue::FieldArray(client_props_array) = client_props_value {
                        // Extraer datos del servidor
                        let mut server_name = String::new();
                        let mut server_version = String::new();
                        let mut server_uuid = String::new();
                        
                        // Iterar sobre los elementos del FieldArray
                        for value in client_props_array.as_slice().iter() {
                            if let AMQPValue::LongString(data_str) = value {
                                let data = data_str.to_string();
                                
                                // Buscar el nombre del servidor
                                if data.contains("irc_server_name") {
                                    // Extraer el valor del elemento actual (segundo <<\" en el string)
                                    if let Some(first_start) = data.find("<<\"") {
                                        if let Some(second_start) = data[first_start + 3..].find("<<\"") {
                                            let actual_start = first_start + 3 + second_start + 3;
                                            if let Some(end) = data[actual_start..].find("\">>") {
                                                server_name = data[actual_start..actual_start + end].to_string();
                                            }
                                        }
                                    }
                                }
                                
                                // Buscar la versión del servidor
                                if data.contains("irc_server_version") {
                                    // Extraer el valor del elemento actual (segundo <<\" en el string)
                                    if let Some(first_start) = data.find("<<\"") {
                                        if let Some(second_start) = data[first_start + 3..].find("<<\"") {
                                            let actual_start = first_start + 3 + second_start + 3;
                                            if let Some(end) = data[actual_start..].find("\">>") {
                                                server_version = data[actual_start..actual_start + end].to_string();
                                            }
                                        }
                                    }
                                }
                                
                                // Buscar el UUID del servidor
                                if data.contains("irc_server_uuid") {
                                    // Extraer el valor del elemento actual (segundo <<\" en el string)
                                    if let Some(first_start) = data.find("<<\"") {
                                        if let Some(second_start) = data[first_start + 3..].find("<<\"") {
                                            let actual_start = first_start + 3 + second_start + 3;
                                            if let Some(end) = data[actual_start..].find("\">>") {
                                                server_uuid = data[actual_start..actual_start + end].to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Mostrar información según el tipo de evento
                        if routing_key == "connection.created" {
                            if !server_name.is_empty() && !server_version.is_empty() && !server_uuid.is_empty() {
                                info!("--> ¡Nueva conexión detectada! Servidor IRC: '{}' (v{}) UUID: {}", server_name, server_version, server_uuid);
                                // Solo agregar si el servidor no existe
                                if !self.servers.read().await.contains_key(&server_uuid) {
                                    if let Ok(uuid) = Uuid::parse_str(&server_uuid) {
                                        self.servers.write().await.insert(
                                            server_uuid.clone(),
                                            ServerInfo {
                                                name: server_name.clone(),
                                                version: server_version.clone(),
                                                uuid,
                                            },
                                        );
                                        let mensaje = format!("{} SERVER {}",
                                            self.server_name, env!("CARGO_PKG_VERSION"));
                                        let _ = self.publish_message(&mensaje).await;
                                    } else {
                                        error!("No se pudo parsear el UUID del servidor: {}", server_uuid);
                                    }
                                }
                            } else {
                                info!("--> ¡Nueva conexión detectada! (datos incompletos)");
                            }
                        } else if routing_key == "connection.closed" {
                            if !server_name.is_empty() && !server_uuid.is_empty() {
                                info!("--> ¡Conexión cerrada detectada! Servidor IRC: '{}' UUID: {}", server_name, server_uuid);
                                if self.servers.read().await.contains_key(&server_uuid) {
                                    self.servers.write().await.remove(&server_uuid);
                                }
                            } else {
                                info!("--> ¡Conexión cerrada detectada! (datos incompletos)");
                            }
                        }
                    } else {
                        info!("    --> client_properties no es un FieldArray: {:?}", client_props_value);
                    }
                } else {
                    info!("    --> No se encontró 'client_properties' en headers");
                }
            } else {
                info!("    --> No hay headers en el mensaje");
            }
            delivery.ack(Default::default()).await?;
        }
        Ok(())
    }

    pub(crate) async fn server_message(&self, message: String) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Manejo de errores más robusto
        let result = match self.parse_server_message(message.clone()) {
            Ok(r) => r,
            Err(e) => {
                error!("Error al parsear mensaje del servidor: {}", e);
                return Err(e);
            }
        };
        let command = result.get_command();
        // Procesar comandos de manera más estructurada
        match command {
            "PRIVMSG" | "NOTICE" => {
                let channel = result.get_text().split_whitespace().nth(0).unwrap_or("");
                let text = result.get_text().split_once(' ').map(|(_, t)| t).unwrap_or("");
                let source = self.parse_user(result.get_user().to_string());
                let snick = source.unwrap().nick.clone();
                // Crear un mensaje que simule venir del servidor
                let server_message = format!("{command} {channel} {text}");
                let state = self.state.read().await;
                // Verificar si el canal existe
                if let Some(chanobj) = state.channels.get(&crate::state::structs::to_unicase(channel)) {
                    let nicks: Vec<String> = chanobj.users.keys().map(|k| k.to_string()).collect();
                    for nick in nicks {
                        if *nick != snick {
                            if let Some(user) = state.users.get(&crate::state::structs::to_unicase(&nick)) {
                                if self.uuid.to_string() != result.get_uuid() {
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
                            if self.uuid.to_string() != result.get_uuid() {
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
                        // Capturar solo los datos necesarios para el spawn, evitando capturar self
                        let channel_name = channel.to_string();
                        let ban_mask_for_timeout = norm_bmask.clone();
                        let state_clone = self.state.clone();
                        let user_str = result.get_user().to_string();
                        let server_message_clone = server_message.clone();
                        let server_uuid = self.uuid.to_string();

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
                                            // Solo notificar a usuarios conectados a este servidor
                                            if server_uuid != result.get_uuid() {
                                                let _ = user.send_msg_display(
                                                    &user_str,
                                                    server_message_clone.as_str()
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
                            if self.uuid.to_string() != result.get_uuid() {
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
            "SERVER" => {
                let info = self.parse_server_message(message.clone()).unwrap();
                let server_uuid = info.get_uuid().to_string();
                let server_name = info.get_user().to_string();
                let server_version = info.get_text().to_string();
                
                if !self.servers.read().await.contains_key(&server_uuid) {
                    self.servers.write().await.insert(
                        server_uuid.to_string(),
                        ServerInfo {
                            name: server_name.to_string(),
                            version: server_version.to_string(),
                            uuid: Uuid::parse_str(&server_uuid).unwrap(),
                        },
                    );
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

        // El formato esperado es :server-uuid user command message
        // Primero, extraer el server-uuid
        let mut rest = &message[1..];
        let uuid_end = rest.find(' ').ok_or("Formato de mensaje inválido: falta UUID")?;
        let uuid = rest[..uuid_end].to_string();
        rest = rest[uuid_end+1..].trim();

        // Extraer el usuario (nick!ident@host)
        let user_end = rest.find(' ').ok_or("Formato de mensaje inválido: falta usuario")?;
        let user = rest[..user_end].to_string();
        rest = rest[user_end+1..].trim();

        // Extraer el comando
        let command_end = rest.find(' ').unwrap_or(rest.len());
        let command = rest[..command_end].trim().to_string();

        // Extraer el texto (si existe)
        let text = if command_end < rest.len() {
            let after_command = rest[command_end..].trim();
            if let Some(stripped) = after_command.strip_prefix(':') {
                stripped.trim().to_string()
            } else {
                after_command.to_string()
            }
        } else {
            String::new()
        };

        Ok(ServMessage {
            uuid,
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
    uuid: String,
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

    pub fn get_user(&self) -> &str {
        &self.user
    }

    pub fn get_uuid(&self) -> &str {
        &self.uuid
    }
}