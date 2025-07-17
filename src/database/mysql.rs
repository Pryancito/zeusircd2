#[cfg(feature = "mysql")]
pub mod mysql_impl {
    use crate::database::{NickDatabase, ChannelDatabase};
    use std::error::Error;
    use std::time::{Duration, SystemTime};
    use async_trait::async_trait;
    use sqlx::mysql::MySqlPoolOptions;
    use sqlx::MySqlPool;

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
        async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            let pool = MySqlPoolOptions::new()
                .max_connections(5)
                .connect(db_config)
                .await?;
            self.pool = Some(pool);
            Ok(())
        }

        async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS nicks (
                        nick VARCHAR(255) PRIMARY KEY,
                        password VARCHAR(255) NOT NULL,
                        user VARCHAR(255) NOT NULL,
                        registration_time BIGINT NOT NULL,
                        email VARCHAR(255),
                        url VARCHAR(255),
                        vhost VARCHAR(255),
                        last_vhost BIGINT,
                        noaccess BOOLEAN NOT NULL DEFAULT FALSE,
                        noop BOOLEAN NOT NULL DEFAULT FALSE,
                        showmail BOOLEAN NOT NULL DEFAULT FALSE
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
                    .execute(pool)
                    .await?;
            }
            Ok(())
        }

        async fn get_nick_info(
            &self,
            nick: &str,
        ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>, Option<String>, Option<SystemTime>, bool, bool, bool)>, Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let row: Option<(String, i64, Option<String>, Option<String>, Option<String>, Option<i64>, bool, bool, bool)> =
                    sqlx::query_as("SELECT user, registration_time, email, url, vhost, last_vhost, noaccess, noop, showmail FROM nicks WHERE nick = ?")
                        .bind(nick)
                        .fetch_optional(pool)
                        .await?;

                if let Some((user, timestamp, email, url, vhost, last_vhost_timestamp, noaccess, noop, showmail)) = row {
                    let registration_time =
                        SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64);
                    let last_vhost = last_vhost_timestamp.map(|ts| 
                        SystemTime::UNIX_EPOCH + Duration::from_secs(ts as u64)
                    );
                    return Ok(Some((user, registration_time, email, url, vhost, last_vhost, noaccess, noop, showmail)));
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
        }

        async fn update_nick_info(
            &mut self,
            nick: &str,
            user: Option<&str>,
            email: Option<&str>,
            url: Option<&str>,
            vhost: Option<&str>,
            last_vhost: Option<SystemTime>,
            noaccess: Option<bool>,
            noop: Option<bool>,
            showmail: Option<bool>,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let mut set_clauses = Vec::new();
                if user.is_some() {
                    set_clauses.push("user = ?");
                }
                if email.is_some() {
                    set_clauses.push("email = ?");
                }
                if url.is_some() {
                    set_clauses.push("url = ?");
                }
                if vhost.is_some() {
                    set_clauses.push("vhost = ?");
                }
                if last_vhost.is_some() {
                    set_clauses.push("last_vhost = ?");
                }
                if noaccess.is_some() {
                    set_clauses.push("noaccess = ?");
                }
                if noop.is_some() {
                    set_clauses.push("noop = ?");
                }
                if showmail.is_some() {
                    set_clauses.push("showmail = ?");
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
                    if let Some(e) = email {
                        query = query.bind(e);
                    }
                    if let Some(u) = url {
                        query = query.bind(u);
                    }
                    if let Some(v) = vhost {
                        query = query.bind(v);
                    }
                    if let Some(lv) = last_vhost {
                        let timestamp = lv.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
                        query = query.bind(timestamp as i64);
                    }
                    if let Some(na) = noaccess {
                        query = query.bind(na);
                    }
                    if let Some(n) = noop {
                        query = query.bind(n);
                    }
                    if let Some(sm) = showmail {
                        query = query.bind(sm);
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
        async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            let pool = MySqlPoolOptions::new()
                .max_connections(5)
                .connect(db_config)
                .await?;
            self.pool = Some(pool);
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
                        topic_setter VARCHAR(255),
                        topic_time BIGINT,
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
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let timestamp = creation_time
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs();
                sqlx::query(
                    "INSERT INTO channels (channel_name, creator_nick, creation_time, modes) VALUES (?, ?, ?, '+r')",
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
        ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>, Option<String>, Option<SystemTime>)>, Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let row: Option<(String, i64, Option<String>, Option<String>, Option<String>, Option<i64>)> = sqlx::query_as(
                    "SELECT creator_nick, creation_time, topic, modes, topic_setter, topic_time FROM channels WHERE channel_name = ?",
                )
                .bind(channel_name)
                .fetch_optional(pool)
                .await?;

                if let Some((creator, timestamp, topic, modes, topic_setter, topic_time_secs)) = row {
                    let creation_time =
                        SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64);
                    let topic_time = topic_time_secs.map(|secs| 
                        SystemTime::UNIX_EPOCH + Duration::from_secs(secs as u64)
                    );
                    return Ok(Some((creator, creation_time, topic, modes, topic_setter, topic_time)));
                }
            }
            Ok(None)
        }

        async fn update_channel_info(
            &mut self,
            channel_name: &str,
            topic: Option<&str>,
            topic_setter: Option<&str>,
            topic_time: Option<SystemTime>,
            modes: Option<&str>,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let mut set_clauses = Vec::new();
                if topic.is_some() {
                    set_clauses.push("topic = ?");
                }
                if topic_setter.is_some() {
                    set_clauses.push("topic_setter = ?");
                }
                if topic_time.is_some() {
                    set_clauses.push("topic_time = ?");
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
                    if let Some(ts) = topic_setter {
                        query = query.bind(ts);
                    }
                    if let Some(tt) = topic_time {
                        let timestamp = tt.duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
                        query = query.bind(timestamp as i64);
                    }
                    if let Some(m) = modes {
                        query = query.bind(m);
                    }
                    query.bind(channel_name).execute(pool).await?;
                }
            }
            Ok(())
        }

        async fn update_channel_owner(
            &mut self,
            channel_name: &str,
            new_owner: &str,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query("UPDATE channels SET creator_nick = ? WHERE channel_name = ?")
                    .bind(new_owner)
                    .bind(channel_name)
                    .execute(pool)
                    .await?;
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

        async fn create_access_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query(
                    "CREATE TABLE IF NOT EXISTS channel_access (
                        channel_name VARCHAR(255) NOT NULL,
                        nick VARCHAR(255) NOT NULL,
                        level VARCHAR(10) NOT NULL,
                        added_by VARCHAR(255) NOT NULL,
                        added_time BIGINT NOT NULL,
                        PRIMARY KEY (channel_name, nick),
                        FOREIGN KEY (channel_name) REFERENCES channels(channel_name) ON DELETE CASCADE
                    )",
                )
                .execute(pool)
                .await?;
            }
            Ok(())
        }

        async fn add_channel_access(
            &mut self,
            channel_name: &str,
            nick: &str,
            level: &str,
            added_by: &str,
            added_time: SystemTime,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let timestamp = added_time
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs();
                sqlx::query(
                    "INSERT INTO channel_access (channel_name, nick, level, added_by, added_time) VALUES (?, ?, ?, ?, ?) ON DUPLICATE KEY UPDATE level = VALUES(level), added_by = VALUES(added_by), added_time = VALUES(added_time)",
                )
                .bind(channel_name)
                .bind(nick)
                .bind(level)
                .bind(added_by)
                .bind(timestamp as i64)
                .execute(pool)
                .await?;
            }
            Ok(())
        }

        async fn get_channel_access(
            &self,
            channel_name: &str,
            nick: &str,
        ) -> Result<Option<(String, String, SystemTime)>, Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let row: Option<(String, String, i64)> = sqlx::query_as(
                    "SELECT level, added_by, added_time FROM channel_access WHERE channel_name = ? AND nick = ?",
                )
                .bind(channel_name)
                .bind(nick)
                .fetch_optional(pool)
                .await?;

                if let Some((level, added_by, timestamp)) = row {
                    let added_time = SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64);
                    return Ok(Some((level, added_by, added_time)));
                }
            }
            Ok(None)
        }

        async fn get_channel_access_list(
            &self,
            channel_name: &str,
            level: Option<&str>,
        ) -> Result<Vec<(String, String, String, SystemTime)>, Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let query = if let Some(_l) = level {
                    "SELECT nick, level, added_by, added_time FROM channel_access WHERE channel_name = ? AND level = ? ORDER BY added_time"
                } else {
                    "SELECT nick, level, added_by, added_time FROM channel_access WHERE channel_name = ? ORDER BY level, added_time"
                };
                
                let mut query_builder = sqlx::query_as::<_, (String, String, String, i64)>(query);
                query_builder = query_builder.bind(channel_name);
                if let Some(l) = level {
                    query_builder = query_builder.bind(l);
                }
                
                let rows = query_builder.fetch_all(pool).await?;
                let mut results = Vec::new();
                for (nick, level, added_by, timestamp) in rows {
                    let added_time = SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp as u64);
                    results.push((nick, level, added_by, added_time));
                }
                return Ok(results);
            }
            Ok(Vec::new())
        }

        async fn update_channel_access(
            &mut self,
            channel_name: &str,
            nick: &str,
            level: &str,
            updated_by: &str,
            updated_time: SystemTime,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                let timestamp = updated_time
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs();
                sqlx::query(
                    "UPDATE channel_access SET level = ?, added_by = ?, added_time = ? WHERE channel_name = ? AND nick = ?",
                )
                .bind(level)
                .bind(updated_by)
                .bind(timestamp as i64)
                .bind(channel_name)
                .bind(nick)
                .execute(pool)
                .await?;
            }
            Ok(())
        }

        async fn delete_channel_access(&mut self, channel_name: &str, nick: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                sqlx::query("DELETE FROM channel_access WHERE channel_name = ? AND nick = ?")
                    .bind(channel_name)
                    .bind(nick)
                    .execute(pool)
                    .await?;
            }
            Ok(())
        }

        async fn migrate_topic_fields(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
            if let Some(pool) = &self.pool {
                // Check if topic_setter column exists
                let check_query = "SELECT COUNT(*) FROM information_schema.columns 
                                  WHERE table_name = 'channels' AND column_name = 'topic_setter'";
                let count: i64 = sqlx::query_scalar(check_query)
                    .fetch_one(pool)
                    .await?;
                
                if count == 0 {
                    sqlx::query("ALTER TABLE channels ADD COLUMN topic_setter VARCHAR(255)")
                        .execute(pool)
                        .await?;
                }
                
                // Check if topic_time column exists
                let check_query = "SELECT COUNT(*) FROM information_schema.columns 
                                  WHERE table_name = 'channels' AND column_name = 'topic_time'";
                let count: i64 = sqlx::query_scalar(check_query)
                    .fetch_one(pool)
                    .await?;
                
                if count == 0 {
                    sqlx::query("ALTER TABLE channels ADD COLUMN topic_time BIGINT")
                        .execute(pool)
                        .await?;
                }
            }
            
            Ok(())
        }
    }
}