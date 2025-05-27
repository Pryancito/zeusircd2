use lapin::{
    options::*,
    types::FieldTable,
    Connection, ConnectionProperties,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::result::Result;

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

pub struct ServerCommunication {
    // Eliminar los campos:
    // connection: Arc<Mutex<Connection>>,
    // channel: Arc<Mutex<lapin::Channel>>,
    // exchange: String,
    // queue: String,
    amqp_url: String,
    exchange: String,
    queue: String,
    connections: std::collections::HashMap<String, Connection>,
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

        Ok(Self {
            // Eliminar los campos:
            // connection: Arc::new(Mutex::new(connection)),
            // channel: Arc::new(Mutex::new(channel)),
            // exchange: exchange.to_string(),
            // queue: queue.to_string(),
            amqp_url: url.to_string(),
            exchange: exchange.to_string(),
            queue: queue.to_string(),
            connections: std::collections::HashMap::new(),
            connected_servers: std::collections::HashSet::new(),
        })
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
        self.connections.insert(server.to_string(), connection);
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
            connection.close(200, "Normal closure").await?;
        }

        // Remover el servidor de la lista de servidores conectados
        self.connected_servers.remove(server);

        Ok(())
    }

    // Eliminar los métodos:
    // pub async fn publish_message(&self, message: ServerMessage) -> Result<()>
    // pub async fn consume_messages<F>(&self, callback: F) -> Result<()>
} 