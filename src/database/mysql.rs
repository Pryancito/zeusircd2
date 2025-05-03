#[cfg(feature = "mysql")]
pub mod mysql_impl {
    use crate::database::{NickDatabase, ChannelDatabase};
    use mysql_async::{prelude::*, Value};
    use mysql_async::{OptsBuilder, Pool};
    use std::error::Error;
    use std::time::SystemTime;
    use async_trait::async_trait;

    pub struct MySqlNickDatabase {
        pub pool: Option<Pool>,
        db_url: String,
    }

    impl MySqlNickDatabase {
        pub fn new(db_url: &str) -> Self {
            MySqlNickDatabase {
                pool: None,
                db_url: db_url.to_string(),
            }
        }
    }

    #[async_trait]
    impl NickDatabase for MySqlNickDatabase {
        async fn connect(&mut self, _db_config: &str) -> Result<(), Box<dyn Error>> {
            let opts = OptsBuilder::from_opts(self.db_url.as_str());
            
            // Use `?` to handle the Result returned by Pool::new
            let pool = Pool::new(opts);
            
            // Assign the pool to `self.pool`
            self.pool = Some(pool);
            
            Ok(())
        }

        async fn create_table(&mut self) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                conn.query_drop(
                    r"CREATE TABLE IF NOT EXISTS nicks (
                        nick VARCHAR(255) PRIMARY KEY,
                        user VARCHAR(255) NOT NULL,
                        registration_time BIGINT UNSIGNED NOT NULL
                    )"
                ).await.map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn add_nick(&mut self, nick: &str, user: &str, registration_time: SystemTime) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let timestamp = registration_time.duration_since(SystemTime::UNIX_EPOCH)
                    .map_err(|_| "Failed to get UNIX timestamp")?
                    .as_secs();
                conn.exec_drop(
                    r"INSERT INTO nicks (nick, user, registration_time)
                    VALUES (?, ?, ?)",
                    (nick, user, timestamp as u64),
                ).await.map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn get_nick_info(&self, nick: &str) -> Result<Option<(String, SystemTime)>, Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let result: Option<(String, u64)> = conn
                    .exec_first(
                        "SELECT user, registration_time FROM nicks WHERE nick = ?",
                        (nick,),
                    )
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn Error>)?;

                match result {
                    Some((user, timestamp)) => {
                        let registration_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
                        Ok(Some((user, registration_time)))
                    }
                    None => Ok(None),
                }
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn update_nick_info(
            &mut self,
            nick: &str,
            user: Option<&str>,
            registration_time: Option<SystemTime>,
        ) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let mut updates = Vec::new();
                let mut params = Vec::new();
        
                if let Some(u) = user {
                    updates.push("user = ?");
                    params.push(Value::from(u)); // Convert to `Value`
                }
                if let Some(rt) = registration_time {
                    let timestamp = rt
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map_err(|_| "Failed to get UNIX timestamp")?
                        .as_secs();
                    updates.push("registration_time = ?");
                    params.push(Value::from(timestamp as u64)); // Convert to `Value`
                }
        
                if updates.is_empty() {
                    return Ok(()); // No updates to perform
                }
        
                let set_clause = updates.join(", ");
                let query = format!("UPDATE nicks SET {} WHERE nick = ?", set_clause);
                params.push(Value::from(nick)); // Add `nick` to the params
        
                conn.exec_drop(query, params).await.map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn delete_nick(&mut self, nick: &str) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                conn.exec_drop(
                    "DELETE FROM nicks WHERE nick = ?",
                    (nick,),
                ).await.map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn close(&mut self) -> Result<(), Box<dyn Error>> {
            self.pool = None;
            Ok(())
        }
    }

    pub struct MySqlChannelDatabase {
        pub pool: Option<Pool>,
        db_url: String,
    }

    impl MySqlChannelDatabase {
        pub fn new(db_url: &str) -> Self {
            MySqlChannelDatabase {
                pool: None,
                db_url: db_url.to_string(),
            }
        }
    }

    #[async_trait]
    impl ChannelDatabase for MySqlChannelDatabase {
        async fn connect(&mut self, _db_config: &str) -> Result<(), Box<dyn Error>> {
            let opts = OptsBuilder::from_opts(self.db_url.as_str());
            
            let pool = Pool::new(opts);
            
            // Assign the pool to `self.pool`
            self.pool = Some(pool);
            
            Ok(())
        }

        async fn create_table(&mut self) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                conn.query_drop(
                    r"CREATE TABLE IF NOT EXISTS channels (
                        channel_name VARCHAR(255) PRIMARY KEY,
                        creator_nick VARCHAR(255) NOT NULL,
                        creation_time BIGINT UNSIGNED NOT NULL,
                        topic TEXT,
                        modes VARCHAR(255)
                    )"
                ).await.map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn add_channel(&mut self, channel_name: &str, creator_nick: &str, creation_time: SystemTime) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let timestamp = creation_time.duration_since(SystemTime::UNIX_EPOCH)
                    .map_err(|_| "Failed to get UNIX timestamp")?
                    .as_secs();
                conn.exec_drop(
                    r"INSERT INTO channels (channel_name, creator_nick, creation_time)
                    VALUES (?, ?, ?)",
                    (channel_name, creator_nick, timestamp as u64),
                ).await.map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn get_channel_info(&self, channel_name: &str) -> Result<Option<(String, SystemTime, Option<String>, Option<String>)>, Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let result: Option<(String, u64, Option<String>, Option<String>)> = conn
                    .exec_first(
                        "SELECT creator_nick, creation_time, topic, modes FROM channels WHERE channel_name = ?",
                        (channel_name,),
                    )
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn Error>)?;

                match result {
                    Some((creator_nick, timestamp, topic, modes)) => {
                        let creation_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp);
                        Ok(Some((creator_nick, creation_time, topic, modes)))
                    }
                    None => Ok(None),
                }
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn update_channel_info(
            &mut self,
            channel_name: &str,
            topic: Option<&str>,
            modes: Option<&str>,
        ) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let mut updates = Vec::new();
                let mut params = Vec::new(); // Changed to Vec<Value>
        
                if let Some(t) = topic {
                    updates.push("topic = ?");
                    params.push(Value::from(t)); // Convert to `Value`
                }
                if let Some(m) = modes {
                    updates.push("modes = ?");
                    params.push(Value::from(m)); // Convert to `Value`
                }
        
                if updates.is_empty() {
                    return Ok(()); // No updates to perform
                }
        
                let set_clause = updates.join(", ");
                let query = format!("UPDATE channels SET {} WHERE channel_name = ?", set_clause);
                params.push(Value::from(channel_name)); // Add `channel_name` to the params
        
                conn.exec_drop(query, params).await.map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn delete_channel(&mut self, channel_name: &str) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let mut conn = pool.get_conn().await.map_err(|e| Box::new(e) as Box<dyn Error>)?;
                conn.exec_drop(
                    "DELETE FROM channels WHERE channel_name = ?",
                    (channel_name,),
                ).await.map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database pool not initialized".into())
            }
        }

        async fn close(&mut self) -> Result<(), Box<dyn Error>> {
            self.pool = None;
            Ok(())
        }
    }
}