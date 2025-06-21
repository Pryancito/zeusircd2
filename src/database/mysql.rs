#[cfg(feature = "mysql")]
pub mod mysql_impl {
    use crate::database::{NickDatabase, ChannelDatabase};
    use std::error::Error;
    use std::time::{Duration, SystemTime};
    use async_trait::async_trait;
    use sqlx::mysql::MySqlPoolOptions;
    use sqlx::MySqlPool;
<<<<<<< HEAD
    use std::time::{Duration, SystemTime};
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)

    pub struct MysqlNickDatabase {
        pool: Option<MySqlPool>,
    }

    impl MysqlNickDatabase {
        pub fn new() -> Self {
            MysqlNickDatabase { pool: None }
        }
    }

    #[async_trait]
    impl NickDatabase for MysqlNickDatabase {
<<<<<<< HEAD
        async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error>> {
=======
        async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
            let pool = MySqlPoolOptions::new()
                .max_connections(5)
                .connect(db_config)
                .await?;
            self.pool = Some(pool);
<<<<<<< HEAD
            Ok(())
        }

        async fn close(&mut self) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = self.pool.take() {
                pool.close().await;
            }
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
            Ok(())
        }

        async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = self.pool.take() {
                pool.close().await;
            }
            Ok(())
        }

        async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS nicks (
                        nick VARCHAR(255) PRIMARY KEY,
                        password VARCHAR(255) NOT NULL,
                        user VARCHAR(255) NOT NULL,
                        registration_time BIGINT NOT NULL
                    )",
                )
                .execute(pool)
                .await?;
            }
            Ok(())
        }

        async fn add_nick(
            &mut self,
            nick: &str,
            password: &str,
            user: &str,
            registration_time: SystemTime,
<<<<<<< HEAD
        ) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let timestamp = registration_time
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs();
                sqlx::query("INSERT INTO nicks (nick, password, user, registration_time) VALUES (?, ?, ?, ?)")
                    .bind(nick)
                    .bind(password)
                    .bind(user)
                    .bind(timestamp as i64)
                    .execute(pool)
                    .await?;
            }
            Ok(())
        }

        async fn get_nick_info(
            &self,
            nick: &str,
        ) -> Result<Option<(String, SystemTime)>, Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let row: Option<(String, i64)> =
                    sqlx::query_as("SELECT user, registration_time FROM nicks WHERE nick = ?")
                        .bind(nick)
                        .fetch_optional(pool)
                        .await?;

                if let Some((user, timestamp)) = row {
                    let registration_time =
                        SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64);
                    return Ok(Some((user, registration_time)));
                }
            }
            Ok(None)
        }

        async fn get_nick_password(&self, nick: &str) -> Result<Option<String>, Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                let row: Option<(String,)> =
                    sqlx::query_as("SELECT password FROM nicks WHERE nick = ?")
                        .bind(nick)
                        .fetch_optional(pool)
                        .await?;
                return Ok(row.map(|(password,)| password));
            }
            Ok(None)
        }

        async fn update_nick_password(&mut self, nick: &str, password: &str) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = &self.pool {
                sqlx::query("UPDATE nicks SET password = ? WHERE nick = ?")
                    .bind(password)
                    .bind(nick)
=======
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let timestamp = registration_time
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs();
                sqlx::query("INSERT INTO nicks (nick, password, user, registration_time) VALUES (?, ?, ?, ?)")
                    .bind(nick)
                    .bind(password)
                    .bind(user)
                    .bind(timestamp as i64)
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
                    .execute(pool)
                    .await?;
            }
            Ok(())
<<<<<<< HEAD
=======
        }

        async fn get_nick_info(
            &self,
            nick: &str,
        ) -> Result<Option<(String, SystemTime)>, Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let row: Option<(String, i64)> =
                    sqlx::query_as("SELECT user, registration_time FROM nicks WHERE nick = ?")
                        .bind(nick)
                        .fetch_optional(pool)
                        .await?;

                if let Some((user, timestamp)) = row {
                    let registration_time =
                        SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64);
                    return Ok(Some((user, registration_time)));
                }
            }
            Ok(None)
        }

        async fn get_nick_password(&self, nick: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let row: Option<(String,)> =
                    sqlx::query_as("SELECT password FROM nicks WHERE nick = ?")
                        .bind(nick)
                        .fetch_optional(pool)
                        .await?;
                return Ok(row.map(|(password,)| password));
            }
            Ok(None)
        }

        async fn update_nick_password(&mut self, nick: &str, password: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query("UPDATE nicks SET password = ? WHERE nick = ?")
                    .bind(password)
                    .bind(nick)
                    .execute(pool)
                    .await?;
            }
            Ok(())
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        }

        async fn update_nick_info(
            &mut self,
            nick: &str,
            user: Option<&str>,
            registration_time: Option<SystemTime>,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let mut set_clauses = Vec::new();
                if user.is_some() {
                    set_clauses.push("user = ?");
                }
                if registration_time.is_some() {
                    set_clauses.push("registration_time = ?");
                }

                if !set_clauses.is_empty() {
                    let query_str = format!(
                        "UPDATE nicks SET {} WHERE nick = ?",
                        set_clauses.join(", ")
                    );
                    let mut query = sqlx::query(&query_str);
                    if let Some(u) = user {
                        query = query.bind(u);
                    }
                    if let Some(rt) = registration_time {
                        let timestamp = rt.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
                        query = query.bind(timestamp as i64);
                    }
                    query.bind(nick).execute(pool).await?;
                }
            }
            Ok(())
        }

        async fn delete_nick(&mut self, nick: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query("DELETE FROM nicks WHERE nick = ?")
                    .bind(nick)
                    .execute(pool)
                    .await?;
            }
            Ok(())
        }
    }

    pub struct MysqlChannelDatabase {
        pool: Option<MySqlPool>,
    }

    impl MysqlChannelDatabase {
        pub fn new() -> Self {
            MysqlChannelDatabase { pool: None }
        }
    }

    #[async_trait]
    impl ChannelDatabase for MysqlChannelDatabase {
<<<<<<< HEAD
        async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error>> {
=======
        async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
            let pool = MySqlPoolOptions::new()
                .max_connections(5)
                .connect(db_config)
                .await?;
            self.pool = Some(pool);
<<<<<<< HEAD
            Ok(())
        }

        async fn close(&mut self) -> Result<(), Box<dyn Error>> {
            if let Some(pool) = self.pool.take() {
                pool.close().await;
            }
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
            Ok(())
        }

        async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = self.pool.take() {
                pool.close().await;
            }
            Ok(())
        }

        async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS channels (
                        channel_name VARCHAR(255) PRIMARY KEY,
                        creator_nick VARCHAR(255) NOT NULL,
                        creation_time BIGINT NOT NULL,
                        topic TEXT,
                        modes TEXT
                    )",
                )
                .execute(pool)
                .await?;
            }
            Ok(())
        }

        async fn add_channel(
            &mut self,
            channel_name: &str,
            creator_nick: &str,
            creation_time: SystemTime,
<<<<<<< HEAD
        ) -> Result<(), Box<dyn Error>> {
=======
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
            if let Some(pool) = &self.pool {
                let timestamp = creation_time
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs();
                sqlx::query(
                    "INSERT INTO channels (channel_name, creator_nick, creation_time) VALUES (?, ?, ?)",
                )
                .bind(channel_name)
                .bind(creator_nick)
                .bind(timestamp as i64)
                .execute(pool)
                .await?;
            }
            Ok(())
        }

        async fn get_channel_info(
            &self,
            channel_name: &str,
<<<<<<< HEAD
        ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>)>, Box<dyn Error>> {
=======
        ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>)>, Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
            if let Some(pool) = &self.pool {
                let row: Option<(String, i64, Option<String>, Option<String>)> = sqlx::query_as(
                    "SELECT creator_nick, creation_time, topic, modes FROM channels WHERE channel_name = ?",
                )
                .bind(channel_name)
                .fetch_optional(pool)
                .await?;

                if let Some((creator, timestamp, topic, modes)) = row {
                    let creation_time =
                        SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64);
                    return Ok(Some((creator, creation_time, topic, modes)));
                }
            }
            Ok(None)
        }

        async fn update_channel_info(
            &mut self,
            channel_name: &str,
            topic: Option<&str>,
            modes: Option<&str>,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let mut set_clauses = Vec::new();
                if topic.is_some() {
                    set_clauses.push("topic = ?");
                }
                if modes.is_some() {
                    set_clauses.push("modes = ?");
                }

                if !set_clauses.is_empty() {
                    let query_str = format!(
                        "UPDATE channels SET {} WHERE channel_name = ?",
                        set_clauses.join(", ")
                    );
                    let mut query = sqlx::query(&query_str);
                    if let Some(t) = topic {
                        query = query.bind(t);
                    }
                    if let Some(m) = modes {
                        query = query.bind(m);
                    }
                    query.bind(channel_name).execute(pool).await?;
                }
            }
            Ok(())
        }

        async fn delete_channel(&mut self, channel_name: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query("DELETE FROM channels WHERE channel_name = ?")
                    .bind(channel_name)
                    .execute(pool)
                    .await?;
            }
            Ok(())
        }
    }
}