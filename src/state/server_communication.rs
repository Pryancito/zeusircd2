use lapin::{
    options::*,
    types::FieldTable,
    Connection, ConnectionProperties,
    BasicProperties,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::result::Result;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::state::*;

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    server: String,
    user: String,
    command: String,
    text: String,
}

#[derive(Clone)]
pub struct ServerCommunication {
    amqp_url: String,
    exchange: String,
    queue: String,
    connections: std::collections::HashMap<String, Arc<Mutex<Connection>>>,
    connected_servers: std::collections::HashSet<String>,
}

impl ServerCommunication {
    pub async fn new(url: &str, exchange: &str, queue: &str) -> Result<Self, Box<dyn Error>> {
        let connection = Connection::connect(
            url,
            ConnectionProperties::default(),
        ).await?;

        let channel = connection.create_channel().await?;

        // Declarar el exchange
        channel.exchange_declare(
            exchange,
            lapin::ExchangeKind::Fanout,
            ExchangeDeclareOptions::default(),
            FieldTable::default(),
        ).await?;

        // Declarar la cola
        channel.queue_declare(
            queue,
            QueueDeclareOptions::default(),
            FieldTable::default(),
        ).await?;

        // Vincular la cola al exchange
        channel.queue_bind(
            queue,
            exchange,
            "",
            QueueBindOptions::default(),
            FieldTable::default(),
        ).await?;

        let server_comm = Self {
            amqp_url: url.to_string(),
            exchange: exchange.to_string(),
            queue: queue.to_string(),
            connections: std::collections::HashMap::new(),
            connected_servers: std::collections::HashSet::new(),
        };

        // Iniciar el consumidor en un task separado
        let server_comm_clone = server_comm.clone();
        tokio::spawn(async move {
            if let Err(e) = server_comm_clone.consume_messages(|message| {
                println!("Mensaje AMQP recibido: {:?}", message);
                Ok(())
            }).await {
                error!("Error en el consumidor AMQP: {:?}", e);
            }
        });

        Ok(server_comm)
    }

    pub async fn connect_server(&mut self, server: &str, _port: u16) -> Result<(), Box<dyn Error>> {
        // Verificar si ya estamos conectados a este servidor
        if self.connected_servers.contains(server) {
            return Err("Already connected to this server".into());
        }

        // Crear la conexión AMQP
        let connection = Connection::connect(&self.amqp_url, ConnectionProperties::default()).await?;
        let channel = connection.create_channel().await?;

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
                &format!("server.{}", server),
                QueueBindOptions::default(),
                FieldTable::default(),
            )
            .await?;

        // Guardar la conexión
        self.connections.insert(server.to_string(), Arc::new(Mutex::new(connection)));
        self.connected_servers.insert(server.to_string());

        Ok(())
    }

    pub async fn disconnect_server(&mut self, server: &str) -> Result<(), Box<dyn Error>> {
        // Verificar si estamos conectados a este servidor
        if !self.connected_servers.contains(server) {
            return Err("Not connected to this server".into());
        }

        // Cerrar la conexión AMQP
        if let Some(connection) = self.connections.remove(server) {
            let close_result = {
                let conn = connection.lock().await;
                conn.close(200, "Normal closure").await
            };
            close_result?;
        }

        // Remover el servidor de la lista de servidores conectados
        self.connected_servers.remove(server);

        Ok(())
    }

    pub async fn publish_message(&self, message: ServerMessage) -> Result<(), Box<dyn Error>> {
        // Serializar el mensaje a JSON
        let message_bytes = serde_json::to_vec(&message)?;
        
        // Publicar el mensaje en todos los servidores conectados
        for (server, connection) in &self.connections {
            let conn = connection.lock().await;
            let channel = conn.create_channel().await?;
            
            // Publicar el mensaje en el exchange
            channel
                .basic_publish(
                    &self.exchange,
                    &format!("server.{}", server),
                    BasicPublishOptions::default(),
                    &message_bytes,
                    BasicProperties::default(),
                )
                .await?;
        }

        Ok(())
    }

    pub async fn consume_messages<F>(&self, callback: F) -> Result<(), Box<dyn Error + Send + Sync>>
    where
        F: Fn(ServerMessage) -> Result<(), Box<dyn Error>> + Send + Sync + 'static,
    {
        // Crear un canal para consumir mensajes
        let connection = Connection::connect(&self.amqp_url, ConnectionProperties::default()).await?;
        let channel = connection.create_channel().await?;

        // Configurar el consumidor
        let mut consumer = channel
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
        let result = self.parse_server_message(message).unwrap();
        println!("Mensaje recibido de: {}", result.server);
    }

    fn parse_server_message(&self, message: String) -> Result<Message, Box<dyn Error>> {
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

        Ok(Message {
            server,
            user,
            command,
            text
        })
    }
} 