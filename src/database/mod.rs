#[cfg(feature = "sqlite")]
pub mod sqlite;
#[cfg(feature = "mysql")]
pub mod mysql;
use std::error::Error;
use std::time::SystemTime;

#[async_trait::async_trait]
pub trait NickDatabase: Send + Sync {
<<<<<<< HEAD
<<<<<<< HEAD
    async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error>>;
    async fn close(&mut self) -> Result<(), Box<dyn Error>>;
    async fn create_table(&mut self) -> Result<(), Box<dyn Error>>;
    async fn add_nick(&mut self, nick: &str, password: &str, user: &str, registration_time: SystemTime) -> Result<(), Box<dyn Error>>;
    async fn get_nick_info(&self, nick: &str) -> Result<Option<(String, SystemTime)>, Box<dyn Error>>;
    async fn get_nick_password(&self, nick: &str) -> Result<Option<String>, Box<dyn Error>>;
    async fn update_nick_password(&mut self, nick: &str, password: &str) -> Result<(), Box<dyn Error>>;
    async fn update_nick_info(&mut self, nick: &str, user: Option<&str>, registration_time: Option<SystemTime>) -> Result<(), Box<dyn Error>>;
    async fn delete_nick(&mut self, nick: &str) -> Result<(), Box<dyn Error>>;
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn add_nick(&mut self, nick: &str, password: &str, user: &str, registration_time: SystemTime) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get_nick_info(&self, nick: &str) -> Result<Option<(String, SystemTime)>, Box<dyn Error + Send + Sync>>;
    async fn get_nick_password(&self, nick: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>>;
    async fn update_nick_password(&mut self, nick: &str, password: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn update_nick_info(&mut self, nick: &str, user: Option<&str>, registration_time: Option<SystemTime>) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn delete_nick(&mut self, nick: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
}

#[async_trait::async_trait]
pub trait ChannelDatabase: Send + Sync {
    async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn add_channel(&mut self, channel_name: &str, creator_nick: &str, creation_time: SystemTime) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get_channel_info(&self, channel_name: &str) -> Result<Option<(String, SystemTime, Option<String>, Option<String>)>, Box<dyn Error + Send + Sync>>;
    async fn update_channel_info(&mut self, channel_name: &str, topic: Option<&str>, modes: Option<&str>) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn delete_channel(&mut self, channel_name: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
}