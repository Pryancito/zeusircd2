// chanserv.rs - ChanServ commands
//
// simple-irc-server - simple IRC server
// Copyright (C) 2022-2024  Mateusz Szpakowski
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

use super::*;
use serde::ser::StdError;
use std::time::SystemTime;

impl super::MainState {
    pub(super) async fn process_chanserv<'a>(
        &self,
        conn_state: &mut ConnState,
        subcommand: &'a str,
        params: Vec<&'a str>,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        let client = conn_state.user_state.client_name();
        let nick = if let Some(nick) = &conn_state.user_state.nick {
            nick
        } else {
            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :No tienes un nick.", client)).await?;
            return Ok(());
        };

        match subcommand.to_lowercase().as_str() {
            "register" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS REGISTER <canal>", client)).await?;
                    return Ok(());
                }
                let channel = params[0];

                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_some() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' ya está registrado.", client, channel)).await?;
                        return Ok(());
                    }

                    db.add_channel(channel, nick, SystemTime::now()).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' ha sido registrado.", client, channel)).await?;
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "drop" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS DROP <canal>", client)).await?;
                    return Ok(());
                }
                let channel = params[0];

                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_some() {
                        db.delete_channel(channel).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' ha sido eliminado.", client, channel)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' no está registrado.", client, channel)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "info" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS INFO <canal>", client)).await?;
                    return Ok(());
                }
                let channel = params[0];

                if let Some(db_arc) = &self.databases.chan_db {
                    let db = db_arc.read().await;
                    if let Some(info) = db.get_channel_info(channel).await? {
                        let creation_time = info.1.duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let datetime = chrono::DateTime::from_timestamp(creation_time as i64, 0)
                            .unwrap_or_default()
                            .format("%Y-%m-%d %H:%M:%S");
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Información del canal {}: Creado por {} el {}", client, channel, info.0, datetime)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' no está registrado.", client, channel)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            _ => {
                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Comando desconocido. Usa /CS HELP.", client)).await?;
            }
        }
        Ok(())
    }
}


