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
                eprintln!("Failed to start daemon: {e}");
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
        initialize_logging(&config);
        let (_, handles) = run_server(config).await?;
        // and await for end
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Error en handles: {e}");
            }
        }
    }
    Ok(())
}