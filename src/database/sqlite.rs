use crate::database::{ChannelDatabase, NickDatabase};
use async_trait::async_trait;
use sqlite::Connection;
use std::error::Error;
use std::sync::Mutex;
use std::time::SystemTime;

pub struct SQLiteNickDatabase {
    connection: Mutex<Connection>,
}

impl SQLiteNickDatabase {
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let conn = Connection::open(db_path)?;
        Ok(SQLiteNickDatabase {
            connection: Mutex::new(conn),
        })
    }
}

impl Drop for SQLiteNickDatabase {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl Drop for SQLiteChannelDatabase {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[async_trait]
impl NickDatabase for SQLiteNickDatabase {
    async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let conn = Connection::open(db_config)?;
        self.connection = Mutex::new(conn);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        drop(self.connection.lock().unwrap());
        Ok(())
    }

    async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        match db_guard.execute(
            "CREATE TABLE IF NOT EXISTS nicks (
                    nick TEXT PRIMARY KEY,
                    password TEXT NOT NULL,
                    user TEXT NOT NULL,
                    registration_time INTEGER NOT NULL,
                    email TEXT,
                    url TEXT,
                    vhost TEXT,
                    last_vhost INTEGER,
                    noaccess INTEGER NOT NULL DEFAULT 0,
                    noop INTEGER NOT NULL DEFAULT 0,
                    showmail INTEGER NOT NULL DEFAULT 0
                )",
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
        }
    }

    async fn add_nick(
        &mut self,
        nick: &str,
        password: &str,
        user: &str,
        registration_time: SystemTime,
        email: Option<&str>,
        url: Option<&str>,
        vhost: Option<&str>,
        last_vhost: Option<SystemTime>,
        noaccess: bool,
        noop: bool,
        showmail: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let timestamp = registration_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| "Failed to get UNIX timestamp")?
            .as_secs();
        let last_vhost_timestamp = last_vhost
            .map(|lv| lv.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()))
            .transpose()
            .map_err(|_| "Failed to get UNIX timestamp for last_vhost")?;
        let query =
            "INSERT INTO nicks (nick, password, user, registration_time, email, url, vhost, last_vhost, noaccess, noop, showmail) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, password)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((3, user)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((4, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((5, email)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((6, url)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((7, vhost)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((8, last_vhost_timestamp.map(|t| t as i64))).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((9, if noaccess { 1 } else { 0 })).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((10, if noop { 1 } else { 0 })).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((11, if showmail { 1 } else { 0 })).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn get_nick_info(
        &self,
        nick: &str,
    ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>, Option<String>, Option<SystemTime>, bool, bool, bool)>, Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "SELECT user, registration_time, email, url, vhost, last_vhost, noaccess, noop, showmail FROM nicks WHERE nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        match statement.next() {
            Ok(sqlite::State::Row) => {
                // Process the row
                let user: String = statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let timestamp_secs: i64 =
                    statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let registration_time =
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                let email: Option<String> = statement.read(2).ok();
                let url: Option<String> = statement.read(3).ok();
                let vhost: Option<String> = statement.read(4).ok();
                let last_vhost_timestamp: Option<i64> = statement.read(5).ok();
                let last_vhost = last_vhost_timestamp.map(|ts| 
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(ts as u64)
                );
                let noaccess: i64 = statement.read(6).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let noop: i64 = statement.read(7).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let showmail: i64 = statement.read(8).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                Ok(Some((user, registration_time, email, url, vhost, last_vhost, noaccess != 0, noop != 0, showmail != 0)))
            }
            Ok(sqlite::State::Done) => Ok(None), // No rows found
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>), // Error from statement.next()
        }
    }

    async fn get_nick_password(&self, nick: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "SELECT password FROM nicks WHERE nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        match statement.next() {
            Ok(sqlite::State::Row) => {
                // Process the row
                let password: String =
                    statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                Ok(Some(password))
            }
            Ok(sqlite::State::Done) => Ok(None), // No rows found
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>), // Error from statement.next()
        }
    }

    async fn update_nick_password(
        &mut self,
        nick: &str,
        password: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();

        let query = format!("UPDATE nicks SET password = ? WHERE nick = ?");
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        statement.bind((1, password)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn update_nick_info(
        &mut self,
        nick: &str,
        user: Option<&str>,
        registration_time: Option<SystemTime>,
        email: Option<&str>,
        url: Option<&str>,
        vhost: Option<&str>,
        last_vhost: Option<SystemTime>,
        noaccess: Option<bool>,
        noop: Option<bool>,
        showmail: Option<bool>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let mut updates = Vec::new();
        if let Some(u) = user {
            updates.push(("user = ?", u.to_string()));
        }
        if let Some(rt) = registration_time {
            let timestamp = rt
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|_| "Failed to get UNIX timestamp")?
                .as_secs();
            updates.push(("registration_time = ?", timestamp.to_string()));
        }
        if let Some(e) = email {
            updates.push(("email = ?", e.to_string()));
        }
        if let Some(u) = url {
            updates.push(("url = ?", u.to_string()));
        }
        if let Some(v) = vhost {
            updates.push(("vhost = ?", v.to_string()));
        }
        if let Some(lv) = last_vhost {
            let timestamp = lv
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|_| "Failed to get UNIX timestamp for last_vhost")?
                .as_secs();
            updates.push(("last_vhost = ?", timestamp.to_string()));
        }
        if let Some(na) = noaccess {
            updates.push(("noaccess = ?", if na { "1" } else { "0" }.to_string()));
        }
        if let Some(n) = noop {
            updates.push(("noop = ?", if n { "1" } else { "0" }.to_string()));
        }
        if let Some(sm) = showmail {
            updates.push(("showmail = ?", if sm { "1" } else { "0" }.to_string()));
        }

        if updates.is_empty() {
            return Ok(());
        }

        let set_clause = updates.iter().map(|(field, _)| *field).collect::<Vec<&str>>().join(", ");
        let query = format!("UPDATE nicks SET {} WHERE nick = ?", set_clause);
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        let mut bind_index = 1;
        for (_, value) in updates {
            statement.bind((bind_index, value.as_str())).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
        statement.bind((bind_index, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn delete_nick(&mut self, nick: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "DELETE FROM nicks WHERE nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }
}

pub struct SQLiteChannelDatabase {
    connection: Mutex<Connection>,
}

impl SQLiteChannelDatabase {
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let conn = Connection::open(db_path)?;
        Ok(SQLiteChannelDatabase {
            connection: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl ChannelDatabase for SQLiteChannelDatabase {
    async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let conn = Connection::open(db_config)?;
        self.connection = Mutex::new(conn);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        drop(self.connection.lock().unwrap());
        Ok(())
    }

    async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        match db_guard.execute(
            "CREATE TABLE IF NOT EXISTS channels (
                    channel_name TEXT PRIMARY KEY,
                    creator_nick TEXT NOT NULL,
                    creation_time INTEGER NOT NULL,
                    topic TEXT,
                    topic_setter TEXT,
                    topic_time INTEGER,
                    modes TEXT
                )",
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
        }
    }

    async fn add_channel(
        &mut self,
        channel_name: &str,
        creator_nick: &str,
        creation_time: SystemTime,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let timestamp = creation_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| "Failed to get UNIX timestamp")?
            .as_secs();
        let query =
            "INSERT INTO channels (channel_name, creator_nick, creation_time, modes) VALUES (?, ?, ?, '+r')";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, creator_nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((3, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn get_channel_info(
        &self,
        channel_name: &str,
    ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>, Option<String>, Option<SystemTime>)>, Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query =
            "SELECT creator_nick, creation_time, topic, modes, topic_setter, topic_time FROM channels WHERE channel_name = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        match statement.next() {
            Ok(sqlite::State::Row) => {
                let creator_nick: String =
                    statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let timestamp_secs: i64 =
                    statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let creation_time =
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                let topic: Option<String> =
                    statement.read(2).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let modes: Option<String> =
                    statement.read(3).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let topic_setter: Option<String> =
                    statement.read(4).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let topic_time_secs: Option<i64> =
                    statement.read(5).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                
                let topic_time = topic_time_secs.map(|secs| 
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64)
                );
                
                Ok(Some((creator_nick, creation_time, topic, modes, topic_setter, topic_time)))
            }
            Ok(sqlite::State::Done) => Ok(None),
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
        }
    }

    async fn update_channel_info(
        &mut self,
        channel_name: &str,
        topic: Option<&str>,
        topic_setter: Option<&str>,
        topic_time: Option<SystemTime>,
        modes: Option<&str>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let mut updates = Vec::new();
        if topic.is_some() {
            updates.push("topic = ?");
        }
        if topic_setter.is_some() {
            updates.push("topic_setter = ?");
        }
        if topic_time.is_some() {
            updates.push("topic_time = ?");
        }
        if modes.is_some() {
            updates.push("modes = ?");
        }

        if updates.is_empty() {
            return Ok(());
        }

        let set_clause = updates.join(", ");
        let query = format!("UPDATE channels SET {} WHERE channel_name = ?", set_clause);
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        let mut bind_index = 1;
        if let Some(t) = topic {
            statement.bind((bind_index, t)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
        if let Some(ts) = topic_setter {
            statement.bind((bind_index, ts)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
        if let Some(tt) = topic_time {
            let timestamp = tt
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|_| "Failed to get UNIX timestamp for topic_time")?
                .as_secs();
            statement.bind((bind_index, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
        if let Some(m) = modes {
            statement.bind((bind_index, m)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
        statement.bind((bind_index, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn update_channel_owner(
        &mut self,
        channel_name: &str,
        new_owner: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "UPDATE channels SET creator_nick = ? WHERE channel_name = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, new_owner)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn delete_channel(&mut self, channel_name: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "DELETE FROM channels WHERE channel_name = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn create_access_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        match db_guard.execute(
            "CREATE TABLE IF NOT EXISTS channel_access (
                    channel_name TEXT NOT NULL,
                    nick TEXT NOT NULL,
                    level TEXT NOT NULL,
                    added_by TEXT NOT NULL,
                    added_time INTEGER NOT NULL,
                    PRIMARY KEY (channel_name, nick),
                    FOREIGN KEY (channel_name) REFERENCES channels(channel_name) ON DELETE CASCADE
                )",
        ) {
            Ok(_) => Ok(()),
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
        }
    }

    async fn add_channel_access(
        &mut self,
        channel_name: &str,
        nick: &str,
        level: &str,
        added_by: &str,
        added_time: SystemTime,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let timestamp = added_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| "Failed to get UNIX timestamp")?
            .as_secs();
        let query =
            "INSERT OR REPLACE INTO channel_access (channel_name, nick, level, added_by, added_time) VALUES (?, ?, ?, ?, ?)";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((3, level)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((4, added_by)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((5, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn get_channel_access(
        &self,
        channel_name: &str,
        nick: &str,
    ) -> Result<Option<(String, String, SystemTime)>, Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "SELECT level, added_by, added_time FROM channel_access WHERE channel_name = ? AND nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        match statement.next() {
            Ok(sqlite::State::Row) => {
                let level: String = statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let added_by: String = statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let timestamp_secs: i64 = statement.read(2).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let added_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                Ok(Some((level, added_by, added_time)))
            }
            Ok(sqlite::State::Done) => Ok(None),
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
        }
    }

    async fn get_channel_access_list(
        &self,
        channel_name: &str,
        level: Option<&str>,
    ) -> Result<Vec<(String, String, String, SystemTime)>, Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = if let Some(_l) = level {
            "SELECT nick, level, added_by, added_time FROM channel_access WHERE channel_name = ? AND level = ? ORDER BY added_time"
        } else {
            "SELECT nick, level, added_by, added_time FROM channel_access WHERE channel_name = ? ORDER BY level, added_time"
        };
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        if let Some(l) = level {
            statement.bind((2, l)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        }

        let mut results = Vec::new();
        while let Ok(sqlite::State::Row) = statement.next() {
            let nick: String = statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            let level: String = statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            let added_by: String = statement.read(2).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            let timestamp_secs: i64 = statement.read(3).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            let added_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
            results.push((nick, level, added_by, added_time));
        }
        Ok(results)
    }

    async fn update_channel_access(
        &mut self,
        channel_name: &str,
        nick: &str,
        level: &str,
        updated_by: &str,
        updated_time: SystemTime,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let timestamp = updated_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| "Failed to get UNIX timestamp")?
            .as_secs();
        let query = "UPDATE channel_access SET level = ?, added_by = ?, added_time = ? WHERE channel_name = ? AND nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, level)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, updated_by)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((3, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((4, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((5, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn delete_channel_access(&mut self, channel_name: &str, nick: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "DELETE FROM channel_access WHERE channel_name = ? AND nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn migrate_topic_fields(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        
        // Check if topic_setter column exists
        let check_query = "PRAGMA table_info(channels)";
        let mut statement = db_guard.prepare(check_query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        
        let mut has_topic_setter = false;
        let mut has_topic_time = false;
        
        while let Ok(sqlite::State::Row) = statement.next() {
            let column_name: String = statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            match column_name.as_str() {
                "topic_setter" => has_topic_setter = true,
                "topic_time" => has_topic_time = true,
                _ => {}
            }
        }
        
        drop(statement);
        
        // Add columns if they don't exist
        if !has_topic_setter {
            db_guard.execute("ALTER TABLE channels ADD COLUMN topic_setter TEXT")
                .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        }
        
        if !has_topic_time {
            db_guard.execute("ALTER TABLE channels ADD COLUMN topic_time INTEGER")
                .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        }
        
        Ok(())
    }
}