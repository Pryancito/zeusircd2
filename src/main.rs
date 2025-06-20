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
#[cfg(any(feature = "mysql", feature = "sqlite"))]
mod database;

use clap::Parser;
use rpassword::prompt_password;
use std::error::Error;
use tracing::error;
#[cfg(unix)]
use daemonize::Daemonize;

use command::*;
use config::*;
use state::*;
use utils::*;

#[cfg(feature = "sqlite")]
use crate::database::sqlite::{SQLiteChannelDatabase, SQLiteNickDatabase};
#[cfg(feature = "mysql")]
use crate::database::mysql::{MySQLNickDatabase, MySQLChannelDatabase};
#[cfg(any(feature = "mysql", feature = "sqlite"))]
use crate::database::{NickDatabase, ChannelDatabase};
#[cfg(any(feature = "mysql", feature = "sqlite"))]
use std::sync::{Arc, Mutex};
#[cfg(any(feature = "mysql", feature = "sqlite"))]
pub struct DBState {
    pub nick_db: Arc<Mutex<Box<dyn NickDatabase + Send + Sync>>>,
    pub channel_db: Arc<Mutex<Box<dyn ChannelDatabase + Send + Sync>>>,
}

#[cfg(any(feature = "mysql", feature = "sqlite"))]
impl DBState {
    pub(crate) async fn new(config: &MainConfig) -> Self {
        let db_config = config.database.as_ref().unwrap();

        let (nick_db, chan_db): (
            Option<Arc<Mutex<dyn NickDatabase + Send + Sync>>>,
            Option<Arc<Mutex<dyn ChannelDatabase + Send + Sync>>>,
        ) = match db_config.db_type.as_str() {
            "sqlite" => {
                #[cfg(feature = "sqlite")]
                {
                    let nick_db = SQLiteNickDatabase::new().expect("failed to open nick database");
                    let chan_db = SQLiteChannelDatabase::new().expect("failed to open channel database");
                    (Some(Arc::new(Mutex::new(nick_db))), Some(Arc::new(Mutex::new(chan_db))))
                }
                #[cfg(not(feature = "sqlite"))]
                panic!("SQLite support is required for SQLite database");
            }
            "mysql" => {
                #[cfg(feature = "mysql")]
                {
                    let nick_db = MySQLNickDatabase::new(&db_config.db_name).await.expect("failed to open nick database");
                    let chan_db = MySQLChannelDatabase::new(&db_config.db_name).await.expect("failed to open channel database");
                    (Some(Arc::new(Mutex::new(nick_db))), Some(Arc::new(Mutex::new(chan_db))))
                }
                #[cfg(not(feature = "mysql"))]
                panic!("MySQL support is required for MySQL database");
            }
            db_type => Err(format!("Unsupported database type: {}", db_type)),
        };

        let mut nick_db = nick_db.expect("Failed to initialize nick database");
        let mut channel_db = chan_db.expect("Failed to initialize channel database");

        nick_db.create_table().await.expect("Failed to create nick table");
        channel_db.create_table().await.expect("Failed to create channel table");

        DBState {
            nick_db: Arc::new(Mutex::new(nick_db)),
            channel_db: Arc::new(Mutex::new(channel_db)),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    
    // Si se solicita demonización, hacerlo ANTES de cualquier inicialización
    #[cfg(unix)]
    if cli.background {
        let daemonize = Daemonize::new()
            .pid_file("./ircd.pid")
            .chown_pid_file(true)
            .working_directory("./");
        match daemonize.start() {
            Ok(_) => {
                println!("ZeusiRCd2 daemon started successfully");
            }
            Err(e) => {
                eprintln!("Failed to start daemon: {}", e);
                std::process::exit(1);
            }
        }
    }

    tokio_main(cli)
}

#[tokio::main(flavor = "multi_thread")]
async fn tokio_main(cli: Cli) -> Result<(), Box<dyn Error>> {
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
        let (_, handles) = run_server(config).await?;
        // and await for end
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Error en handles: {}", e);
            }
        }
    }
    Ok(())
}