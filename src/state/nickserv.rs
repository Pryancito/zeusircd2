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

                    db.add_nick(nick, &password_hash, &conn_state.user_state.source, SystemTime::now(), None, None, None, false, false, false).await?;
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
            "email" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Falta el email.", client)).await?;
                    return Ok(());
                }

                let email = params[0];
                
                // Validar formato de email
                if !email.contains('@') || !email.contains('.') {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Formato de email inválido.", client)).await?;
                    return Ok(());
                }

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        // Actualizar el email
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), Some(email), info.3.as_deref(), info.4.as_deref(), Some(info.5), Some(info.6), Some(info.7)).await?;
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu email ha sido actualizado a: {}", client, email)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu nick no está registrado.", client)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "url" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Falta la URL.", client)).await?;
                    return Ok(());
                }

                let url = params[0];
                
                // Validar formato de URL básico
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La URL debe comenzar con http:// o https://", client)).await?;
                    return Ok(());
                }

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        // Actualizar la URL
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), Some(url), info.4.as_deref(), Some(info.5), Some(info.6), Some(info.7)).await?;
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu URL ha sido actualizada a: {}", client, url)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu nick no está registrado.", client)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
                
            }
            "noaccess" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS NOACCESS <on|off>", client)).await?;
                    return Ok(());
                }

                let action = params[0];
                
                // Validar la acción (on/off)
                let new_noaccess = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Acción inválida. Usa 'on' o 'off'.", client)).await?;
                        return Ok(());
                    }
                };

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick objetivo está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        // Actualizar el estado de noaccess
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), Some(new_noaccess), Some(info.6), Some(info.7)).await?;
                        
                        let status = if new_noaccess { "activada" } else { "desactivada" };
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La opción noaccess para {} ha sido {}.", client, nick, status)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick {} no está registrado.", client, nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "noop" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS NOOP <nick> <on|off>", client)).await?;
                    return Ok(());
                }

                let action = params[0];
                
                // Validar el nick objetivo
                if let Err(_) = validate_username(nick) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Nick inválido.", client)).await?;
                    return Ok(());
                }

                // Validar la acción (on/off)
                let new_noop = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Acción inválida. Usa 'on' o 'off'.", client)).await?;
                        return Ok(());
                    }
                };

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick objetivo está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        // Actualizar el estado de noop
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), Some(info.5), Some(new_noop), Some(info.7)).await?;
                        
                        let status = if new_noop { "activada" } else { "desactivada" };
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La opción noop para {} ha sido {}.", client, nick, status)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick {} no está registrado.", client, nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "showmail" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS SHOWMAIL <on|off>", client)).await?;
                    return Ok(());
                }

                let action = params[0];
                
                // Validar el nick objetivo
                // Validar la acción (on/off)
                let new_showmail = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Acción inválida. Usa 'on' o 'off'.", client)).await?;
                        return Ok(());
                    }
                };

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick objetivo está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        // Actualizar el estado de showmail
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), Some(info.5), Some(info.6), Some(new_showmail)).await?;
                        
                        let status = if new_showmail { "activada" } else { "desactivada" };
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La opción showmail para {} ha sido {}.", client, nick, status)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick {} no está registrado.", client, nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "password" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS PASSWORD <password>", client)).await?;
                    return Ok(());
                }

                let new_password = params[0];
                
                // Validar que la contraseña no esté vacía
                if new_password.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña no puede estar vacía.", client)).await?;
                    return Ok(());
                }

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Cambiar la contraseña
                    db.update_nick_password(nick, new_password).await?;
                    
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña para {} ha sido cambiada exitosamente.", client, nick)).await?;
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
