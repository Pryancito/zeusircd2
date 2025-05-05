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

impl DBState {
    pub(crate) async fn new(config: &MainConfig) -> Self {
        let dbconf = config.database.as_ref().clone();
        let mut nick_db: Box<dyn NickDatabase + Send + Sync> = match dbconf.unwrap().database.as_str() {
            #[cfg(feature = "sqlite")]
            "sqlite" => {
                let path = dbconf.unwrap().url.as_str();
                let mut db = SQLiteNickDatabase::new(&path);
                db.connect("").await.expect("Failed to connect to SQLite");
                Box::new(db)
            }
            #[cfg(feature = "mysql")]
            "mysql" => {
                let url = dbconf.unwrap().url.as_str();
                let mut db = MySqlNickDatabase::new(&url);
                db.connect("").await.expect("Failed to connect to MySQL");
                Box::new(db)
            }
            _ => panic!("Unsupported database type: {}", dbconf.unwrap().database),
        };

        let mut channel_db: Box<dyn ChannelDatabase + Send + Sync> = match dbconf.unwrap().database.as_str() {
            #[cfg(feature = "sqlite")]
            "sqlite" => {
                let path = dbconf.unwrap().url.as_str();
                let mut db = SQLiteChannelDatabase::new(&path);
                db.connect("").await.expect("Failed to connect to SQLite");
                Box::new(db)
            }
            #[cfg(feature = "mysql")]
            "mysql" => {
                let url = dbconf.unwrap().url.as_str();
                let mut db = MySqlChannelDatabase::new(&url);
                db.connect("").await.expect("Failed to connect to MySQL");
                Box::new(db)
            }
            _ => panic!("Unsupported database type: {}", dbconf.unwrap().database),
        };

        nick_db.create_table().await.expect("Failed to create nick table");
        channel_db.create_table().await.expect("Failed to create channel table");

        DBState {
            nick_db: Arc::new(Mutex::new(nick_db)),
            channel_db: Arc::new(Mutex::new(channel_db)),
        }
    }
}

#[tokio::main]
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
        let _ = DBState::new(&config).await;
        initialize_logging(&config);
        // get handle of server
        let (_, handle) = run_server(config).await?;
        // and await for end
        handle.await?;
    }
    Ok(())
}
