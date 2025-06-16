// main.rs - main program
//
// simple-irc-server - simple IRC server
// Copyright (C) 2022  Mateusz Szpakowski
//
// This library is free software; you can redistribute it and/or
// modify it under the terms of the GNU Lesser General Public
// License as published by the Free Software Foundation; either
// version 2.1 of the License, or (at your option) any later version.
//
// This library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
// Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public
// License along with this library; if not, write to the Free Software
// Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA  02110-1301  USA

mod command;
mod config;
mod help;
mod reply;
mod state;
mod utils;
mod database;

use clap::Parser;
use rpassword::prompt_password;
use std::error::Error;

use command::*;
use config::*;
use state::*;
use utils::*;

#[cfg(feature = "sqlite")]
use crate::database::sqlite::sqlite_impl::{SQLiteNickDatabase, SQLiteChannelDatabase};
#[cfg(feature = "mysql")]
use crate::database::mysql::mysql_impl::{MySqlNickDatabase, MySqlChannelDatabase};
use crate::database::{NickDatabase, ChannelDatabase};
use std::sync::{Arc, Mutex};
pub struct DBState {
    pub nick_db: Arc<Mutex<Box<dyn NickDatabase + Send + Sync>>>,
    pub channel_db: Arc<Mutex<Box<dyn ChannelDatabase + Send + Sync>>>,
}

#[cfg(any(feature = "mysql", feature = "sqlite"))]
impl DBState {
    pub(crate) async fn new(config: &MainConfig) -> Self {
        let db_config_option = &config.database;

        if let Some(db_config) = db_config_option {
            let _nick_db: Result<Box<dyn NickDatabase + Send + Sync>, String> = match db_config.database.as_str() {
                #[cfg(feature = "sqlite")]
                "sqlite" => {
                    let path = &db_config.url;
                    let mut db = SQLiteNickDatabase::new(path);
                    let _ = db.connect("")
                        .await
                        .map_err(|e| format!("Failed to connect to SQLite: {}", e));
                    Ok(Box::new(db))
                }
                #[cfg(feature = "mysql")]
                "mysql" => {
                    let url = &db_config.url;
                    let mut db = MySqlNickDatabase::new(url);
                    let _ = db.connect("")
                        .await
                        .map_err(|e| format!("Failed to connect to MySQL: {}", e));
                    Ok(Box::new(db))
                }
                db_type => Err(format!("Unsupported database type: {}", db_type)),
            };

            let _channel_db: Result<Box<dyn ChannelDatabase + Send + Sync>, String> = match db_config.database.as_str() {
                #[cfg(feature = "sqlite")]
                "sqlite" => {
                    let path = &db_config.url;
                    let mut db = SQLiteChannelDatabase::new(path);
                    let _ = db.connect("")
                        .await
                        .map_err(|e| format!("Failed to connect to SQLite: {}", e));
                    Ok(Box::new(db))
                }
                #[cfg(feature = "mysql")]
                "mysql" => {
                    let url = &db_config.url;
                    let mut db = MySqlChannelDatabase::new(url);
                    let _ = db.connect("")
                        .await
                        .map_err(|e| format!("Failed to connect to MySQL: {}", e));
                    Ok(Box::new(db))
                }
                db_type => Err(format!("Unsupported database type: {}", db_type)),
            };

            let mut nick_db = _nick_db.expect("Failed to initialize nick database");
            let mut channel_db = _channel_db.expect("Failed to initialize channel database");

            nick_db.create_table().await.expect("Failed to create nick table");
            channel_db.create_table().await.expect("Failed to create channel table");

            DBState {
                nick_db: Arc::new(Mutex::new(nick_db)),
                channel_db: Arc::new(Mutex::new(channel_db)),
            }
        } else {
            panic!("Database configuration is missing."); // Or handle this more gracefully
        }
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    if cli.gen_password_hash {
        let password = if let Some(pwd) = cli.password {
            pwd
        } else {
            prompt_password("Enter password:")?
        };
        println!("Password Hash: {}", argon2_hash_password(&password));
    } else {
        let config = MainConfig::new(cli)?;
        #[cfg(any(feature = "mysql", feature = "sqlite"))]
        let _ = DBState::new(&config).await;
        initialize_logging(&config);
        let (_, handle) = run_server(config).await?;
        // and await for end
        handle.await?;
    }
    Ok(())
}