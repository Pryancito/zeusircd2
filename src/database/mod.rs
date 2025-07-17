#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(feature = "mysql")]
pub mod mysql;

use std::error::Error;
use std::time::SystemTime;

#[async_trait::async_trait]
pub trait NickDatabase: Send + Sync {
    async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn add_nick(&mut self, nick: &str, password: &str, user: &str, registration_time: SystemTime) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get_nick_info(&self, nick: &str) -> Result<Option<(String, SystemTime, Option<String>, Option<String>, Option<String>, Option<SystemTime>, bool, bool, bool)>, Box<dyn Error + Send + Sync>>;
    async fn get_nick_password(&self, nick: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>>;
    async fn update_nick_password(&mut self, nick: &str, password: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn update_nick_info(&mut self, nick: &str, user: Option<&str>, registration_time: Option<SystemTime>, email: Option<&str>, url: Option<&str>, vhost: Option<&str>, last_vhost: Option<SystemTime>, noaccess: Option<bool>, noop: Option<bool>, showmail: Option<bool>) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn delete_nick(&mut self, nick: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
}

#[async_trait::async_trait]
pub trait ChannelDatabase: Send + Sync {
    async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn add_channel(&mut self, channel_name: &str, creator_nick: &str, creation_time: SystemTime) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get_channel_info(&self, channel_name: &str) -> Result<Option<(String, SystemTime, Option<String>, Option<String>, Option<String>, Option<SystemTime>)>, Box<dyn Error + Send + Sync>>;
    async fn update_channel_info(&mut self, channel_name: &str, topic: Option<&str>, topic_setter: Option<&str>, topic_time: Option<SystemTime>, modes: Option<&str>) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn update_channel_owner(&mut self, channel_name: &str, new_owner: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn delete_channel(&mut self, channel_name: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    
    // Funciones para manejo de acceso de canales
    async fn create_access_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn add_channel_access(&mut self, channel_name: &str, nick: &str, level: &str, added_by: &str, added_time: SystemTime) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get_channel_access(&self, channel_name: &str, nick: &str) -> Result<Option<(String, String, SystemTime)>, Box<dyn Error + Send + Sync>>;
    async fn get_channel_access_list(&self, channel_name: &str, level: Option<&str>) -> Result<Vec<(String, String, String, SystemTime)>, Box<dyn Error + Send + Sync>>;
    async fn update_channel_access(&mut self, channel_name: &str, nick: &str, level: &str, updated_by: &str, updated_time: SystemTime) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn delete_channel_access(&mut self, channel_name: &str, nick: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    
    // Migration function
    async fn migrate_topic_fields(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
}