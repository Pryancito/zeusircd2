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

#[derive(Debug, Serialize, Deserialize)]
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
                // Aquí procesas cada mensaje recibido desde AMQP
                println!("Mensaje AMQP recibido: {:?}", message);
                Ok(())
            }).await {
                eprintln!("Error en el consumidor AMQP: {:?}", e);
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

    pub async fn consume_messages<F>(&self, callback: F) -> Result<(), Box<dyn Error>>
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
                        if let Err(e) = callback(message) {
                            eprintln!("Error procesando mensaje: {:?}", e);
                        }
                    }

                    // Envía el ACK al servidor AMQP
                    let _ = delivery
                        .ack(lapin::options::BasicAckOptions::default())
                        .await;
                }
                Err(e) => {
                    eprintln!("Error recibiendo mensaje de AMQP: {:?}", e);
                }
            }
        }

        Ok(())
    }
} 