use lapin::{
    options::*,
    types::FieldTable,
    Connection, ConnectionProperties, Result,
};
use serde::{Deserialize, Serialize};

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
}

impl ServerCommunication {
    pub async fn new(url: &str, exchange: &str, queue: &str) -> Result<Self> {
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
        })
    }

    // Eliminar los métodos:
    // pub async fn publish_message(&self, message: ServerMessage) -> Result<()>
    // pub async fn consume_messages<F>(&self, callback: F) -> Result<()>
} 