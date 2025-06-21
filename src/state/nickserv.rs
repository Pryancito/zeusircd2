// nickserv.rs - NickServ commands
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
use crate::utils::argon2_hash_password;

impl super::MainState {
    pub(super) async fn process_nickserv<'a>(
        &self,
        conn_state: &mut ConnState,
        subcommand: &'a str,
        params: Vec<&'a str>,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        let client = conn_state.user_state.client_name();
        let nick = if let Some(nick) = &conn_state.user_state.nick {
            nick
        } else {
            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :No tienes un nick.", client)).await?;
            return Ok(());
        };

        match subcommand.to_lowercase().as_str() {
            "register" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Falta la contraseña.", client)).await?;
                    return Ok(());
                }
                let password = params[0];

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    if db.get_nick_info(nick).await?.is_some() {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick '{}' ya está registrado.", client, nick)).await?;
                        return Ok(());
                    }

                    let password_hash = argon2_hash_password(password);

                    db.add_nick(nick, &password_hash, &conn_state.user_state.source, SystemTime::now()).await?;
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick '{}' ha sido registrado.", client, nick)).await?;
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "drop" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Falta la contraseña o el nick.", client)).await?;
                    return Ok(());
                }

                let param = params[0];
                
                // Verificar si el usuario es operador
                let state = self.state.read().await;
                let user = state.users.get(nick).unwrap();
                let is_oper = user.modes.is_local_oper();

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    if is_oper {
                        // Operador puede borrar cualquier nick
                        if db.get_nick_info(param).await?.is_some() {
                            db.delete_nick(param).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick '{}' ha sido eliminado por un operador.", client, param)).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick '{}' no existe.", client, param)).await?;
                        }
                    } else {
                        // Usuario normal debe proporcionar contraseña
                        if let Some(nick_password) = db.get_nick_password(nick).await? {
                            if argon2_hash_password(param) == nick_password {
                                db.delete_nick(nick).await?;
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu nick '{}' ha sido eliminado.", client, nick)).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Contraseña incorrecta.", client)).await?;
                            }
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu nick no está registrado.", client)).await?;
                        }
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            _ => {
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Comando desconocido. Usa /NS HELP.", client)).await?;
            }
        }

        Ok(())
    }
}
