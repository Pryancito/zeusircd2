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
<<<<<<< HEAD
<<<<<<< HEAD
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error>> {
=======
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let conn = Connection::open(db_path)?;
        Ok(SQLiteNickDatabase {
            connection: Mutex::new(conn),
        })
<<<<<<< HEAD
<<<<<<< HEAD
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    }
}

#[async_trait]
impl NickDatabase for SQLiteNickDatabase {
    async fn connect(&mut self, db_config: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let conn = Connection::open(db_config)?;
        self.connection = Mutex::new(conn);
        Ok(())
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    }
}

<<<<<<< HEAD
#[async_trait]
impl NickDatabase for SQLiteNickDatabase {
    async fn create_table(&mut self) -> Result<(), Box<dyn Error>> {
=======
    async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        drop(self.connection.lock().unwrap());
        Ok(())
    }

    async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
    }

    async fn close(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        drop(self.connection.lock().unwrap());
        Ok(())
    }

    async fn create_table(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let db_guard = self.connection.lock().unwrap();
        match db_guard.execute(
            "CREATE TABLE IF NOT EXISTS nicks (
                    nick TEXT PRIMARY KEY,
                    password TEXT NOT NULL,
                    user TEXT NOT NULL,
                    registration_time INTEGER NOT NULL
                )",
        ) {
            Ok(_) => Ok(()),
<<<<<<< HEAD
<<<<<<< HEAD
            Err(e) => Err(Box::new(e) as Box<dyn Error>),
=======
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        }
    }

    async fn add_nick(
        &mut self,
        nick: &str,
        password: &str,
        user: &str,
        registration_time: SystemTime,
<<<<<<< HEAD
<<<<<<< HEAD
    ) -> Result<(), Box<dyn Error>> {
=======
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let db_guard = self.connection.lock().unwrap();
        let timestamp = registration_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| "Failed to get UNIX timestamp")?
            .as_secs();
        let query =
            "INSERT INTO nicks (nick, password, user, registration_time) VALUES (?, ?, ?, ?)";
<<<<<<< HEAD
<<<<<<< HEAD
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((2, password)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((3, user)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((4, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, password)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((3, user)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((4, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    }

    async fn get_nick_info(
        &self,
        nick: &str,
<<<<<<< HEAD
<<<<<<< HEAD
    ) -> Result<Option<(String, SystemTime)>, Box<dyn Error>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "SELECT user, registration_time FROM nicks WHERE nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    ) -> Result<Option<(String, SystemTime)>, Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "SELECT user, registration_time FROM nicks WHERE nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)

        match statement.next() {
            Ok(sqlite::State::Row) => {
                // Process the row
<<<<<<< HEAD
<<<<<<< HEAD
                let user: String = statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let timestamp_secs: i64 =
                    statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error>)?;
=======
                let user: String = statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let timestamp_secs: i64 =
                    statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
                let user: String = statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let timestamp_secs: i64 =
                    statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
                let registration_time =
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                Ok(Some((user, registration_time)))
            }
            Ok(sqlite::State::Done) => Ok(None), // No rows found
<<<<<<< HEAD
<<<<<<< HEAD
            Err(e) => Err(Box::new(e) as Box<dyn Error>), // Error from statement.next()
        }
    }

    async fn get_nick_password(&self, nick: &str) -> Result<Option<String>, Box<dyn Error>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "SELECT password FROM nicks WHERE nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
=======
=======
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
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>), // Error from statement.next()
        }
    }

<<<<<<< HEAD
    async fn get_nick_password(&self, nick: &str) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "SELECT password FROM nicks WHERE nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)

        match statement.next() {
            Ok(sqlite::State::Row) => {
                // Process the row
                let password: String =
<<<<<<< HEAD
                    statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                Ok(Some(password))
            }
            Ok(sqlite::State::Done) => Ok(None), // No rows found
            Err(e) => Err(Box::new(e) as Box<dyn Error>), // Error from statement.next()
=======
                    statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                Ok(Some(password))
            }
            Ok(sqlite::State::Done) => Ok(None), // No rows found
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>), // Error from statement.next()
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        }
    }

=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    async fn update_nick_password(
        &mut self,
        nick: &str,
        password: &str,
<<<<<<< HEAD
<<<<<<< HEAD
    ) -> Result<(), Box<dyn Error>> {
        let db_guard = self.connection.lock().unwrap();

        let query = format!("UPDATE nicks SET password = ? WHERE nick = ?");
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error>)?;

        statement.bind((1, password)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((2, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();

        let query = format!("UPDATE nicks SET password = ? WHERE nick = ?");
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        statement.bind((1, password)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    }

    async fn update_nick_info(
        &mut self,
        nick: &str,
        user: Option<&str>,
        registration_time: Option<SystemTime>,
<<<<<<< HEAD
<<<<<<< HEAD
    ) -> Result<(), Box<dyn Error>> {
=======
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
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

        if updates.is_empty() {
            return Ok(());
        }

        let set_clause = updates.iter().map(|(field, _)| *field).collect::<Vec<&str>>().join(", ");
        let query = format!("UPDATE nicks SET {} WHERE nick = ?", set_clause);
<<<<<<< HEAD
<<<<<<< HEAD
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error>)?;

        let mut bind_index = 1;
        for (_, value) in updates {
            statement.bind((bind_index, value.as_str())).map_err(|e| Box::new(e) as Box<dyn Error>)?;
            bind_index += 1;
        }
        statement.bind((bind_index, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
    }

    async fn delete_nick(&mut self, nick: &str) -> Result<(), Box<dyn Error>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "DELETE FROM nicks WHERE nick = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
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
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    }
}

pub struct SQLiteChannelDatabase {
    connection: Mutex<Connection>,
}

impl SQLiteChannelDatabase {
<<<<<<< HEAD
<<<<<<< HEAD
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error>> {
=======
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
    pub fn new(db_path: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let conn = Connection::open(db_path)?;
        Ok(SQLiteChannelDatabase {
            connection: Mutex::new(conn),
        })
    }
}

#[async_trait]
impl ChannelDatabase for SQLiteChannelDatabase {
<<<<<<< HEAD
<<<<<<< HEAD
    async fn create_table(&mut self) -> Result<(), Box<dyn Error>> {
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
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
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let db_guard = self.connection.lock().unwrap();
        match db_guard.execute(
            "CREATE TABLE IF NOT EXISTS channels (
                    channel_name TEXT PRIMARY KEY,
                    creator_nick TEXT NOT NULL,
                    creation_time INTEGER NOT NULL,
                    topic TEXT,
                    modes TEXT
                )",
        ) {
            Ok(_) => Ok(()),
<<<<<<< HEAD
<<<<<<< HEAD
            Err(e) => Err(Box::new(e) as Box<dyn Error>),
=======
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        }
    }

    async fn add_channel(
        &mut self,
        channel_name: &str,
        creator_nick: &str,
        creation_time: SystemTime,
<<<<<<< HEAD
<<<<<<< HEAD
    ) -> Result<(), Box<dyn Error>> {
=======
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let db_guard = self.connection.lock().unwrap();
        let timestamp = creation_time
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| "Failed to get UNIX timestamp")?
            .as_secs();
        let query =
            "INSERT INTO channels (channel_name, creator_nick, creation_time) VALUES (?, ?, ?)";
<<<<<<< HEAD
<<<<<<< HEAD
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((2, creator_nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((3, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((2, creator_nick)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((3, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    }

    async fn get_channel_info(
        &self,
        channel_name: &str,
<<<<<<< HEAD
<<<<<<< HEAD
    ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>)>, Box<dyn Error>> {
        let db_guard = self.connection.lock().unwrap();
        let query =
            "SELECT creator_nick, creation_time, topic, modes FROM channels WHERE channel_name = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
=======
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>)>, Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query =
            "SELECT creator_nick, creation_time, topic, modes FROM channels WHERE channel_name = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)

        match statement.next() {
            Ok(sqlite::State::Row) => {
                let creator_nick: String =
<<<<<<< HEAD
<<<<<<< HEAD
                    statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let timestamp_secs: i64 =
                    statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let creation_time =
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                let topic: Option<String> =
                    statement.read(2).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                let modes: Option<String> =
                    statement.read(3).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                Ok(Some((creator_nick, creation_time, topic, modes)))
            }
            Ok(sqlite::State::Done) => Ok(None),
            Err(e) => Err(Box::new(e) as Box<dyn Error>),
=======
                    statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let timestamp_secs: i64 =
                    statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let creation_time =
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                let topic: Option<String> =
                    statement.read(2).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let modes: Option<String> =
                    statement.read(3).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                Ok(Some((creator_nick, creation_time, topic, modes)))
            }
            Ok(sqlite::State::Done) => Ok(None),
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
                    statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let timestamp_secs: i64 =
                    statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let creation_time =
                    SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                let topic: Option<String> =
                    statement.read(2).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                let modes: Option<String> =
                    statement.read(3).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
                Ok(Some((creator_nick, creation_time, topic, modes)))
            }
            Ok(sqlite::State::Done) => Ok(None),
            Err(e) => Err(Box::new(e) as Box<dyn Error + Send + Sync>),
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        }
    }

    async fn update_channel_info(
        &mut self,
        channel_name: &str,
        topic: Option<&str>,
        modes: Option<&str>,
<<<<<<< HEAD
<<<<<<< HEAD
    ) -> Result<(), Box<dyn Error>> {
=======
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        let db_guard = self.connection.lock().unwrap();
        let mut updates = Vec::new();
        if topic.is_some() {
            updates.push("topic = ?");
        }
        if modes.is_some() {
            updates.push("modes = ?");
        }

        if updates.is_empty() {
            return Ok(());
        }

<<<<<<< HEAD
<<<<<<< HEAD
        let set_clause = updates.iter().map(|(field, _)| *field).collect::<Vec<&str>>().join(", ");
        let query = format!("UPDATE channels SET {} WHERE channel_name = ?", set_clause);
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error>)?;

        let mut bind_index = 1;
        if let Some(t) = topic {
            statement.bind((bind_index, t)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
            bind_index += 1;
        }
        if let Some(m) = modes {
            statement.bind((bind_index, m)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
            bind_index += 1;
        }
        statement.bind((bind_index, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error>)?;

        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
    }

    async fn delete_channel(&mut self, channel_name: &str) -> Result<(), Box<dyn Error>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "DELETE FROM channels WHERE channel_name = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
=======
        let set_clause = updates.join(", ");
        let query = format!("UPDATE channels SET {} WHERE channel_name = ?", set_clause);
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        let mut bind_index = 1;
        if let Some(t) = topic {
            statement.bind((bind_index, t)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
        if let Some(m) = modes {
            statement.bind((bind_index, m)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
=======
        let set_clause = updates.join(", ");
        let query = format!("UPDATE channels SET {} WHERE channel_name = ?", set_clause);
        let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        let mut bind_index = 1;
        if let Some(t) = topic {
            statement.bind((bind_index, t)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
        if let Some(m) = modes {
            statement.bind((bind_index, m)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
            bind_index += 1;
        }
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
        statement.bind((bind_index, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    async fn delete_channel(&mut self, channel_name: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let db_guard = self.connection.lock().unwrap();
        let query = "DELETE FROM channels WHERE channel_name = ?";
        let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
<<<<<<< HEAD
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
=======
>>>>>>> 5c86584 (next step to database integration. Now register/drop works ok.)
    }
}