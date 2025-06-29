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
            "vop" | "hop" | "aop" | "sop" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS {} <canal> <add|del|list> [nick]", client, subcommand)).await?;
                    return Ok(());
                }
                let channel = params[0];
                let action = params[1].to_lowercase();
                
                // Verificar que el canal existe
                if let Some(db_arc) = &self.databases.chan_db {
                    let db = db_arc.read().await;
                    if db.get_channel_info(channel).await?.is_none() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' no está registrado.", client, channel)).await?;
                        return Ok(());
                    }
                    drop(db);
                    
                    let mut db = db_arc.write().await;
                    
                    match action.as_str() {
                        "add" => {
                            if params.len() < 3 {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS {} {} <canal> add <nick>", client, subcommand, channel)).await?;
                                return Ok(());
                            }
                            let target_nick = params[2];
                            
                            // Verificar si ya existe el acceso
                            if let Some(existing) = db.get_channel_access(channel, target_nick).await? {
                                if existing.0 == subcommand.to_lowercase() {
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :{} ya tiene acceso {} en {}.", client, target_nick, subcommand.to_uppercase(), channel)).await?;
                                } else {
                                    // Actualizar el nivel de acceso
                                    db.update_channel_access(channel, target_nick, &subcommand.to_lowercase(), nick, SystemTime::now()).await?;
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El nivel de acceso de {} en {} ha sido actualizado a {}.", client, target_nick, channel, subcommand.to_uppercase())).await?;
                                }
                            } else {
                                // Agregar nuevo acceso
                                db.add_channel_access(channel, target_nick, &subcommand.to_lowercase(), nick, SystemTime::now()).await?;
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :{} ha sido agregado a la lista {} de {}.", client, target_nick, subcommand.to_uppercase(), channel)).await?;
                            }
                        }
                        "del" => {
                            if params.len() < 3 {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS {} {} <canal> del <nick>", client, subcommand, channel)).await?;
                                return Ok(());
                            }
                            let target_nick = params[2];
                            
                            // Verificar si existe el acceso
                            if let Some(existing) = db.get_channel_access(channel, target_nick).await? {
                                if existing.0 == subcommand.to_lowercase() {
                                    db.delete_channel_access(channel, target_nick).await?;
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :{} ha sido removido de la lista {} de {}.", client, target_nick, subcommand.to_uppercase(), channel)).await?;
                                } else {
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :{} no tiene acceso {} en {}.", client, target_nick, subcommand.to_uppercase(), channel)).await?;
                                }
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :{} no tiene acceso {} en {}.", client, target_nick, subcommand.to_uppercase(), channel)).await?;
                            }
                        }
                        "list" => {
                            let access_list = db.get_channel_access_list(channel, Some(&subcommand.to_lowercase())).await?;
                            if access_list.is_empty() {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :No hay usuarios en la lista {} de {}.", client, subcommand.to_uppercase(), channel)).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Lista {} de {}:", client, subcommand.to_uppercase(), channel)).await?;
                                for (nick, _level, added_by, added_time) in access_list {
                                    let datetime = chrono::DateTime::from_timestamp(added_time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs() as i64, 0)
                                        .unwrap_or_default()
                                        .format("%Y-%m-%d %H:%M:%S");
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :  {} (agregado por {} el {})", client, nick, added_by, datetime)).await?;
                                }
                            }
                        }
                        _ => {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Acción desconocida. Usa: add, del, o list.", client)).await?;
                        }
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


