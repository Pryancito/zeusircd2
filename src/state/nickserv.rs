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
use crate::utils::validate_username;
use std::ops::DerefMut;

impl super::MainState {
    pub(super) async fn process_nickserv<'a>(
        &self,
        conn_state: &mut ConnState,
        subcommand: &'a str,
        params: Vec<&'a str>,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        let client = conn_state.user_state.client_name().to_string();
        let nick = if let Some(nick) = &conn_state.user_state.nick {
            nick
        } else {
            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :You don't have a nick.")).await?;
            return Ok(());
        };

        match subcommand.to_lowercase().as_str() {
            "register" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Password is required.")).await?;
                    return Ok(());
                }
                let password = params[0];
                
                // Validate password
                if password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Password must be at least 6 characters long.")).await?;
                    return Ok(());
                }
                
                if password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Password cannot be longer than 32 characters.")).await?;
                    return Ok(());
                }
                
                // Check that password doesn't contain disallowed characters
                if !password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Password contains disallowed characters.")).await?;
                    return Ok(());
                }
                
                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    if db.get_nick_info(nick).await?.is_some() {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Nick '{nick}' is already registered.")).await?;
                        return Ok(());
                    }

                    let password_hash = argon2_hash_password(password);

                    db.add_nick(nick, &password_hash, &conn_state.user_state.source, SystemTime::now()).await?;
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Nick '{nick}' has been registered.")).await?;
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "drop" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Password or nick is required.")).await?;
                    return Ok(());
                }

                let param = params[0];
                
                // Check if user is operator
                let state = self.state.read().await;
                let user = state.users.get(&crate::state::structs::to_unicase(nick)).unwrap();
                let is_oper = user.modes.is_local_oper();

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    if is_oper {
                        // Operator can delete any nick
                        if db.get_nick_info(param).await?.is_some() {
                            db.delete_nick(param).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Nick '{param}' has been deleted by an operator.")).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Nick '{param}' does not exist.")).await?;
                        }
                    } else {
                        // Normal user must provide password
                        if let Some(nick_password) = db.get_nick_password(nick).await? {
                            if argon2_hash_password(param) == nick_password {
                                db.delete_nick(nick).await?;
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Your nick '{nick}' has been deleted.")).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Incorrect password.")).await?;
                            }
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Your nick is not registered.")).await?;
                        }
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "email" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Usage: /NS EMAIL <email> or /NS EMAIL OFF")).await?;
                    return Ok(());
                }

                let email = params[0];
                
                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Check if nick is registered
                    if let Some(info) = db.get_nick_info(nick).await? {
                        if email.to_lowercase() == "off" {
                            // Disable email
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), None, info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Your email has been disabled.")).await?;
                        } else {
                            // Validate email format
                            if !email.contains('@') || !email.contains('.') {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Invalid email format.")).await?;
                                return Ok(());
                            }
                            
                            // Update email
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(email), info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Your email has been updated to: {email}")).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Your nick is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "url" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Usage: /NS URL <url> or /NS URL OFF")).await?;
                    return Ok(());
                }

                let url = params[0];
                
                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Check if nick is registered
                    if let Some(info) = db.get_nick_info(nick).await? {
                        if url.to_lowercase() == "off" {
                            // Disable URL
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), info.2.as_deref(), None, info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Your URL has been disabled.")).await?;
                        } else {
                            // Validate basic URL format
                            if !url.starts_with("http://") && !url.starts_with("https://") {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :URL must start with http:// or https://")).await?;
                                return Ok(());
                            }
                            
                            // Update URL
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), info.2.as_deref(), Some(url), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Your URL has been updated to: {url}")).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Your nick is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "noaccess" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Usage: /NS NOACCESS <on|off>")).await?;
                    return Ok(());
                }

                let action = params[0];
                
                // Validar la acción (on/off)
                let new_noaccess = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Invalid action. Use 'on' or 'off'.")).await?;
                        return Ok(());
                    }
                };

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Check if nick is registered
                    if let Some(info) = db.get_nick_info(nick).await? {
                        // Actualizar el estado de noaccess
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), info.5, Some(new_noaccess), Some(info.6), Some(info.7)).await?;
                        
                        let status = if new_noaccess { "activada" } else { "desactivada" };
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The noaccess option for {nick} has been {status}.")).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The nick {nick} is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The database is not configured.")).await?;
                }
            }
            "noop" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Usage: /NS NOOP <nick> <on|off>")).await?;
                    return Ok(());
                }

                let target_nick = params[0];
                let action = params[1];
                
                // Validar el nick objetivo
                if validate_username(target_nick).is_err() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Invalid nick.")).await?;
                    return Ok(());
                }

                // Validar la acción (on/off)
                let new_noop = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Invalid action. Use 'on' or 'off'.")).await?;
                        return Ok(());
                    }
                };

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick objetivo está registrado
                    if let Some(info) = db.get_nick_info(target_nick).await? {
                        // Actualizar el estado de noop
                        db.update_nick_info(target_nick, Some(conn_state.user_state.source.as_str()), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(new_noop), Some(info.7)).await?;
                        
                        let status = if new_noop { "activada" } else { "desactivada" };
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The noop option for {target_nick} has been {status}.")).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The nick {target_nick} is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The database is not configured.")).await?;
                }
            }
            "showmail" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Usage: /NS SHOWMAIL <on|off>")).await?;
                    return Ok(());
                }

                let action = params[0];
                
                // Validar el nick objetivo
                // Validar la acción (on/off)
                let new_showmail = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Invalid action. Use 'on' or 'off'.")).await?;
                        return Ok(());
                    }
                };

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick objetivo está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        // Actualizar el estado de showmail
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(new_showmail)).await?;
                        
                        let status = if new_showmail { "activada" } else { "desactivada" };
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The showmail option for {nick} has been {status}.")).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The nick {nick} is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The database is not configured.")).await?;
                }
            }
            "password" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Usage: /NS PASSWORD <password>")).await?;
                    return Ok(());
                }

                let new_password = params[0];
                
                // Validar la contraseña
                if new_password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The password must be at least 6 characters long.")).await?;
                    return Ok(());
                }
                
                if new_password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The password cannot be more than 32 characters long.")).await?;
                    return Ok(());
                }
                
                // Verificar que la contraseña no contenga caracteres no permitidos
                if !new_password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The password contains invalid characters.")).await?;
                    return Ok(());
                }
                
                // Validar que la contraseña no esté vacía
                if new_password.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The password cannot be empty.")).await?;
                    return Ok(());
                }

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Cambiar la contraseña
                    db.update_nick_password(nick, new_password).await?;
                    
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The password for {nick} has been changed successfully.")).await?;
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The database is not configured.")).await?;
                }
                
            }
            "vhost" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Usage: /NS VHOST <off|tu.ip.virtual>")).await?;
                    return Ok(());
                }

                let vhost_action = params[0];
                let new_vhost = if vhost_action == "off" {
                    None
                } else {
                    Some(vhost_action.to_string())
                };

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        // Verificar si han pasado 24 horas desde el último cambio
                        if let Some(last_vhost_change) = info.5 {
                            let now = SystemTime::now();
                            if let Ok(duration) = now.duration_since(last_vhost_change) {
                                if duration.as_secs() < 86400 { // 24 horas en segundos
                                    let remaining = 86400 - duration.as_secs();
                                    let hours = remaining / 3600;
                                    let minutes = (remaining % 3600) / 60;
                                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :You must wait {hours}h {minutes}m before changing your vhost again.")).await?;
                                    return Ok(());
                                }
                            }
                        }
                        
                        // Actualizar el vhost y el timestamp
                        let now = SystemTime::now();
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), info.2.as_deref(), info.3.as_deref(), new_vhost.as_deref(), Some(now), Some(info.6), Some(info.7), Some(info.8)).await?;
                        
                        // Actualizar el vhost en el estado de la conexión y en el estado global
                        if new_vhost.is_some() {
                            conn_state.user_state.set_cloack(new_vhost.clone().expect("ERROR.in.vHost"));
                        }
                        
                        let status = if new_vhost.is_some() { 
                            format!("configurado a {}", new_vhost.as_ref().unwrap())
                        } else { 
                            "desactivado".to_string()
                        };
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Tu vhost ha sido {status}.")).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The nick {nick} is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The database is not configured.")).await?;
                }
            }
            "identify" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Usage: /NS IDENTIFY <nickname> <password>")).await?;
                    return Ok(());
                }

                let target_nick = params[0];
                let password = params[1];
                
                // Validar la contraseña
                if password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The password must be at least 6 characters long.")).await?;
                    return Ok(());
                }
                
                if password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The password cannot be more than 32 characters long.")).await?;
                    return Ok(());
                }
                
                // Verificar que la contraseña no contenga caracteres no permitidos
                if !password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The password contains invalid characters.")).await?;
                    return Ok(());
                }
                
                // Validar el nickname objetivo
                if validate_username(target_nick).is_err() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Invalid nick.")).await?;
                    return Ok(());
                }
                
                if let Some(db_arc) = &self.databases.nick_db {
                    let db = db_arc.read().await;
                    if let Some(nick_password) = db.get_nick_password(target_nick).await? {
                        // Verificar la contraseña
                        if argon2_verify_password_async(password.to_string(), nick_password).await.is_ok() {
                            // Contraseña correcta, obtener vhost y proceder.
                            let nick_info = db.get_nick_info(target_nick).await?;
                            let vhost = if let Some(info) = nick_info {
                                info.4.clone()
                            } else {
                                None
                            };
                            
                            drop(db); // Liberar el lock de la base de datos
                            
                            // Verificar si el nick está en uso y desconectar si es necesario
                            let user_to_disconnect = {
                                let state = self.state.read().await;
                                if state.users.contains_key(&crate::state::structs::to_unicase(target_nick)) {
                                    // El nick está en uso, preparar para desconectar
                                    Some(target_nick.to_string())
                                } else {
                                    None
                                }
                            };
                            
                            // Desconectar al usuario existente si es necesario
                            if let Some(existing_nick) = user_to_disconnect {
                                let mut state = self.state.write().await;
                                if let Some(user) = state.users.remove(&crate::state::structs::to_unicase(&existing_nick)) {
                                    // Enviar mensaje de desconexión al usuario existente
                                    if let Some(sender) = user.quit_sender {
                                        let _ = sender.send((existing_nick.clone(), "NickServ: Nick claimed".to_string()));
                                    }
                                }
                            }
                            
                            // Cambiar el nick del usuario actual
                            let old_nick = nick.clone();
                            let old_source = conn_state.user_state.source.clone();
                            // Actualizar el nick en el estado del usuario
                            conn_state.user_state.set_nick(target_nick.to_string());
                            conn_state.user_state.password = Some(password.to_string());
                            
                            // Actualizar en el estado global
                            let mut statem = self.state.write().await;
                            let state = statem.deref_mut();
                            if let Some(mut user) = state.users.remove(&crate::state::structs::to_unicase(&old_nick)) {
                                if vhost.is_some() {
                                    conn_state.user_state.cloack = vhost.clone().expect("ERROR.in.vHost");
                                    user.cloack = vhost.clone().expect("ERROR.in.vHost");
                                    conn_state.user_state.update_source();
                                }
                                user.update_nick(&conn_state.user_state);
                                if !user.modes.registered {
                                    for channel in &user.channels {
                                        if let Some(chanobj) = state.channels.get_mut(&crate::state::structs::to_unicase(&channel.clone())) {
                                            let nicks: Vec<String> = chanobj.users.keys().cloned().map(|nick| nick.to_string()).collect();
                                            for nicknames in nicks {
                                                if nicknames != target_nick && nicknames != old_nick {
                                                    if let Some(user) = state.users.get_mut(&crate::state::structs::to_unicase(&nicknames)) {
                                                        let part_msg = format!("PART {channel} :vHost");
                                                        let _ = user.send_msg_display(
                                                            &old_source,
                                                            part_msg.as_str()
                                                        );
                                                        let join_msg = format!("JOIN :{channel}");
                                                        let _ = user.send_msg_display(
                                                            &conn_state.user_state.source,
                                                            join_msg.as_str()
                                                        );
                                                        if let Some(user_chum) = chanobj.users.get(&crate::state::structs::to_unicase(&old_nick)) {
                                                            let mut arg = Vec::new();
                                                            if user_chum.founder {
                                                                arg.push("q");
                                                            } if user_chum.protected {
                                                                arg.push("a");
                                                            } if user_chum.operator {
                                                                arg.push("o");
                                                            } if user_chum.half_oper {
                                                                arg.push("h");
                                                            } if user_chum.voice {
                                                                arg.push("v");
                                                            }
                                                            for mode in &arg {
                                                                let msg = format!("MODE {channel} +{mode} {target_nick}");
                                                                let _ = user.send_msg_display(
                                                                    &self.config.name,
                                                                    msg.as_str(),
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                user.modes.registered = true;
                                // Actualizar canales
                                for ch in &user.channels {
                                    if let Some(channel) = state.channels.get_mut(&crate::state::structs::to_unicase(&ch.clone())) {
                                        channel.rename_user(&old_nick, target_nick.to_string());
                                    }
                                }
                                
                                // Actualizar wallops
                                if state.wallops_users.contains(&old_nick) {
                                    state.wallops_users.remove(&old_nick);
                                    state.wallops_users.insert(target_nick.to_string());
                                }
                                
                                // Agregar a la historia
                                state.insert_to_nick_history(&old_nick, user.history_entry.clone());
                                
                                // Insertar con el nuevo nick
                                state.users.insert(crate::state::structs::to_unicase(target_nick), user.clone());
                            }
                            
                            // Obtener el nuevo client_name después de las modificaciones
                            let new_client = conn_state.user_state.client_name();
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {new_client} :You have been successfully identified as {target_nick}.")).await?;
                            
                            // Notificar a todos los usuarios sobre el cambio de nick
                            let nick_change_msg = format!("NICK :{target_nick}");
                            for u in state.users.values() {
                                let _ = u.send_msg_display(&old_source, nick_change_msg.clone());
                            }
                            
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Incorrect password.")).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The nick {target_nick} is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The database is not configured.")).await?;
                }
            }
            "info" => {
                if let Some(db_arc) = &self.databases.nick_db {
                    let db = db_arc.read().await;
                    if let Some((user, registration_date, email, url, vhost, last_vhost, noaccess, noop, showmail)) = db.get_nick_info(nick).await? {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Nick information: {nick}")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :User: {user}")).await?;
                        if let Some(vhost) = &vhost {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Vhost: {vhost}")).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Vhost: Not configured")).await?;
                        }
                        if let Some(last_vhost) = &last_vhost {
                            if let Ok(duration) = SystemTime::now().duration_since(*last_vhost) {
                                let days = duration.as_secs() / 86400;
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Last vhost change: {days} days")).await?;
                            }
                        }
                        if showmail {
                            if let Some(email) = &email {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Email: {email}")).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Email: No set")).await?;
                            }
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Email: Not shown")).await?;
                        }
                        if let Some(url) = &url {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :URL: {url}")).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :URL: No set")).await?;
                        }
                        if let Ok(duration) = SystemTime::now().duration_since(registration_date) {
                            let days = duration.as_secs() / 86400;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Registered {days} days ago")).await?;
                        }
                        if noaccess {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :No access: Enabled")).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :No access: Disabled")).await?;
                        }
                        if noop { 
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :No op: Enabled")).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :No op: Disabled")).await?;
                        }
                        if showmail {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Showmail: Enabled")).await?;  
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Showmail: Disabled")).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :The nick {nick} is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "help" => {
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :NickServ commands:")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  REGISTER <password> - Register your nick")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  DROP <password> - Delete your nick registration")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  EMAIL <email|OFF> - Set or disable your email")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  URL <url|OFF> - Set or disable your URL")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  VHOST <vhost|OFF> - Set or disable your vhost")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  NOACCESS <on|off> - Enable or disable no access mode")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  NOOP <on|off> - Enable or disable no op mode")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  SHOWMAIL <on|off> - Enable or disable showmail mode")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  PASSWORD <password> - Change your password")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  IDENTIFY <nickname> <password> - Identify yourself to the server")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  INFO [nick] - Show nick information")).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :  HELP - Show available commands")).await?;
            }
            _ => {
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {client} :Unknown command. Use /NS HELP for available commands.")).await?;
            }
        }
        Ok(())
    }
}
