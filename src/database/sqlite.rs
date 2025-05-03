#[cfg(feature = "sqlite")]
pub mod sqlite_impl {
    use crate::database::{NickDatabase, ChannelDatabase};
    use sqlite::Connection;
    use std::error::Error;
    use std::time::SystemTime;
    use async_trait::async_trait;
    use std::sync::Mutex;

    pub struct SQLiteNickDatabase {
        connection: Option<Mutex<Connection>>,
        db_path: String,
    }

    impl SQLiteNickDatabase {
        pub fn new(db_path: &str) -> Self {
            SQLiteNickDatabase {
                connection: None,
                db_path: db_path.to_string(),
            }
        }
    }

    #[async_trait]
    impl NickDatabase for SQLiteNickDatabase {
        async fn connect(&mut self, _db_config: &str) -> Result<(), Box<dyn Error>> {
            match Connection::open(&self.db_path) {
                Ok(conn) => {
                    self.connection = Some(Mutex::new(conn));
                    Ok(())
                }
                Err(e) => Err(Box::new(e)),
            }
        }

        async fn close(&mut self) -> Result<(), Box<dyn Error>> {
            self.connection = None;
            Ok(())
        }

        async fn create_table(&mut self) -> Result<(), Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                match db_guard.execute(
                    "CREATE TABLE IF NOT EXISTS nicks (
                        nick TEXT PRIMARY KEY,
                        user TEXT NOT NULL,
                        registration_time INTEGER NOT NULL
                    )"
                ) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(Box::new(e) as Box<dyn Error>),
                }
            } else {
                Err("Database not connected".into())
            }
        }

        async fn add_nick(&mut self, nick: &str, user: &str, registration_time: SystemTime) -> Result<(), Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                let timestamp = registration_time.duration_since(SystemTime::UNIX_EPOCH)
                    .map_err(|_| "Failed to get UNIX timestamp")?
                    .as_secs();
                let query = "INSERT INTO nicks (nick, user, registration_time) VALUES (?, ?, ?)";
                let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((2, user)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((3, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database not connected".into())
            }
        }

        async fn get_nick_info(
            &self,
            nick: &str,
        ) -> Result<Option<(String, SystemTime)>, Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                let query = "SELECT user, registration_time FROM nicks WHERE nick = ?";
                let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        
                match statement.next() {
                    Ok(sqlite::State::Row) => {
                        // Process the row
                        let user: String = statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                        let timestamp_secs: i64 = statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                        let registration_time =
                            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                        Ok(Some((user, registration_time)))
                    }
                    Ok(sqlite::State::Done) => Ok(None), // No rows found
                    Err(e) => Err(Box::new(e) as Box<dyn Error>), // Error from statement.next()
                }
            } else {
                Err("Database not connected".into())
            }
        }

        async fn update_nick_info(&mut self, nick: &str, user: Option<&str>, registration_time: Option<SystemTime>) -> Result<(), Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                let mut updates = Vec::new();
                if let Some(u) = user {
                    updates.push(("user = ?", u.to_string()));
                }
                if let Some(rt) = registration_time {
                    let timestamp = rt.duration_since(SystemTime::UNIX_EPOCH)
                        .map_err(|_| "Failed to get UNIX timestamp")?
                        .as_secs();
                    updates.push(("registration_time = ?", timestamp.to_string()));
                }
        
                if updates.is_empty() {
                    return Ok(());
                }
        
                let set_clause = updates.iter().map(|(field, _)| *field).collect::<Vec<&str>>().join(", ");
                let query = format!("UPDATE nicks SET {} WHERE nick = ?", set_clause);
                let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        
                let mut bind_index = 1;
                for (_, value) in updates {
                    statement.bind((bind_index, value.as_str())).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                    bind_index += 1;
                }
                statement.bind((bind_index, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database not connected".into())
            }
        }

        async fn delete_nick(&mut self, nick: &str) -> Result<(), Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                let query = "DELETE FROM nicks WHERE nick = ?";
                let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((1, nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database not connected".into())
            }
        }
    }

    pub struct SQLiteChannelDatabase {
        connection: Option<Mutex<Connection>>,
        db_path: String,
    }

    impl SQLiteChannelDatabase {
        pub fn new(db_path: &str) -> Self {
            SQLiteChannelDatabase {
                connection: None,
                db_path: db_path.to_string(),
            }
        }
    }

    #[async_trait]
    impl ChannelDatabase for SQLiteChannelDatabase {
        async fn connect(&mut self, _db_config: &str) -> Result<(), Box<dyn Error>> {
            match Connection::open(&self.db_path) {
                Ok(conn) => {
                    self.connection = Some(Mutex::new(conn));
                    Ok(())
                }
                Err(e) => Err(Box::new(e)),
            }
        }

        async fn close(&mut self) -> Result<(), Box<dyn Error>> {
            self.connection = None;
            Ok(())
        }

        async fn create_table(&mut self) -> Result<(), Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                match db_guard.execute(
                        "CREATE TABLE IF NOT EXISTS channels (
                            channel_name TEXT PRIMARY KEY,
                            creator_nick TEXT NOT NULL,
                            creation_time INTEGER NOT NULL,
                            topic TEXT,
                            modes TEXT
                        )"
                ) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(Box::new(e) as Box<dyn Error>),
                }
            } else {
                Err("Database not connected".into())
            }
        }

        async fn add_channel(&mut self, channel_name: &str, creator_nick: &str, creation_time: SystemTime) -> Result<(), Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                let timestamp = creation_time.duration_since(SystemTime::UNIX_EPOCH)
                    .map_err(|_| "Failed to get UNIX timestamp")?
                    .as_secs();
                let query = "INSERT INTO channels (channel_name, creator_nick, creation_time) VALUES (?, ?, ?)";
                let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((2, creator_nick)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((3, timestamp as i64)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database not connected".into())
            }
        }

        async fn get_channel_info(
            &self,
            channel_name: &str,
        ) -> Result<Option<(String, SystemTime, Option<String>, Option<String>)>, Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                let query = "SELECT creator_nick, creation_time, topic, modes FROM channels WHERE channel_name = ?";
                let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        
                // Manejar el Result devuelto por `statement.next()`
                match statement.next() {
                    Ok(sqlite::State::Row) => {
                        // Procesar la fila si el estado es `Row`
                        let creator_nick: String = statement.read(0).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                        let timestamp_secs: i64 = statement.read(1).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                        let creation_time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(timestamp_secs as u64);
                        let topic: sqlite::Result<String> = statement.read(2);
                        let modes: sqlite::Result<String> = statement.read(3);
        
                        // Retornar los resultados
                        return Ok(Some((
                            creator_nick,
                            creation_time,
                            topic.ok(),
                            modes.ok(),
                        )));
                    }
                    Ok(sqlite::State::Done) => {
                        // Si no se encuentran filas, retornar `Ok(None)`
                        return Ok(None);
                    }
                    Err(e) => {
                        // Manejar errores al intentar obtener la siguiente fila
                        eprintln!("Error al iterar sobre las filas: {:?}", e);
                        Err(Box::new(e) as Box<dyn Error>)
                    }
                }
            } else {
                // Si no hay conexión a la base de datos
                Err("Database not connected".into())
            }
        }
        
        async fn update_channel_info(&mut self, channel_name: &str, topic: Option<&str>, modes: Option<&str>) -> Result<(), Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                let mut updates = Vec::new();
                if let Some(t) = topic {
                    updates.push(("topic = ?", t.to_string()));
                }
                if let Some(m) = modes {
                    updates.push(("modes = ?", m.to_string()));
                }
        
                if updates.is_empty() {
                    return Ok(());
                }
        
                let set_clause = updates.iter().map(|(field, _)| *field).collect::<Vec<&str>>().join(", ");
                let query = format!("UPDATE channels SET {} WHERE channel_name = ?", set_clause);
                let mut statement = db_guard.prepare(&query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        
                let mut bind_index = 1;
                for (_, value) in updates {
                    statement.bind((bind_index, value.as_str())).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                    bind_index += 1;
                }
                statement.bind((bind_index, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database not connected".into())
            }
        }
        
        async fn delete_channel(&mut self, channel_name: &str) -> Result<(), Box<dyn Error>> {
            if let Some(conn_mutex) = &self.connection {
                let db_guard = conn_mutex.lock().unwrap();
                let query = "DELETE FROM channels WHERE channel_name = ?";
                let mut statement = db_guard.prepare(query).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.bind((1, channel_name)).map_err(|e| Box::new(e) as Box<dyn Error>)?;
                statement.next().map(|_| ()).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("Database not connected".into())
            }
        }
    }
}