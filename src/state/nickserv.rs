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
            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :You don't have a nick.", client)).await?;
            return Ok(());
        };

        match subcommand.to_lowercase().as_str() {
            "register" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Password is required.", client)).await?;
                    return Ok(());
                }
                let password = params[0];
                
                // Validate password
                if password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Password must be at least 6 characters long.", client)).await?;
                    return Ok(());
                }
                
                if password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Password cannot be longer than 32 characters.", client)).await?;
                    return Ok(());
                }
                
                // Check that password doesn't contain disallowed characters
                if !password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Password contains disallowed characters.", client)).await?;
                    return Ok(());
                }
                
                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    if db.get_nick_info(nick).await?.is_some() {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Nick '{}' is already registered.", client, nick)).await?;
                        return Ok(());
                    }

                    let password_hash = argon2_hash_password(password);

                    db.add_nick(nick, &password_hash, &conn_state.user_state.source, SystemTime::now()).await?;
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Nick '{}' has been registered.", client, nick)).await?;
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Database is not configured.", client)).await?;
                }
            }
            "drop" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Password or nick is required.", client)).await?;
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
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Nick '{}' has been deleted by an operator.", client, param)).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Nick '{}' does not exist.", client, param)).await?;
                        }
                    } else {
                        // Normal user must provide password
                        if let Some(nick_password) = db.get_nick_password(nick).await? {
                            if argon2_hash_password(param) == nick_password {
                                db.delete_nick(nick).await?;
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Your nick '{}' has been deleted.", client, nick)).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Incorrect password.", client)).await?;
                            }
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Your nick is not registered.", client)).await?;
                        }
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Database is not configured.", client)).await?;
                }
            }
            "email" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Usage: /NS EMAIL <email> or /NS EMAIL OFF", client)).await?;
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
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Your email has been disabled.", client)).await?;
                        } else {
                            // Validate email format
                            if !email.contains('@') || !email.contains('.') {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Invalid email format.", client)).await?;
                                return Ok(());
                            }
                            
                            // Update email
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(email), info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Your email has been updated to: {}", client, email)).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Your nick is not registered.", client)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Database is not configured.", client)).await?;
                }
            }
            "url" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Usage: /NS URL <url> or /NS URL OFF", client)).await?;
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
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Your URL has been disabled.", client)).await?;
                        } else {
                            // Validate basic URL format
                            if !url.starts_with("http://") && !url.starts_with("https://") {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :URL must start with http:// or https://", client)).await?;
                                return Ok(());
                            }
                            
                            // Update URL
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), info.2.as_deref(), Some(url), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Your URL has been updated to: {}", client, url)).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Your nick is not registered.", client)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Database is not configured.", client)).await?;
                }
            }
            "noaccess" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Usage: /NS NOACCESS <on|off>", client)).await?;
                    return Ok(());
                }

                let action = params[0];
                
                // Validar la acción (on/off)
                let new_noaccess = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Invalid action. Use 'on' or 'off'.", client)).await?;
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
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The noaccess option for {} has been {}.", client, nick, status)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The nick {} is not registered.", client, nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The database is not configured.", client)).await?;
                }
            }
            "noop" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Usage: /NS NOOP <nick> <on|off>", client)).await?;
                    return Ok(());
                }

                let target_nick = params[0];
                let action = params[1];
                
                // Validar el nick objetivo
                if let Err(_) = validate_username(target_nick) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Invalid nick.", client)).await?;
                    return Ok(());
                }

                // Validar la acción (on/off)
                let new_noop = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Invalid action. Use 'on' or 'off'.", client)).await?;
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
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The noop option for {} has been {}.", client, target_nick, status)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The nick {} is not registered.", client, target_nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The database is not configured.", client)).await?;
                }
            }
            "showmail" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Usage: /NS SHOWMAIL <on|off>", client)).await?;
                    return Ok(());
                }

                let action = params[0];
                
                // Validar el nick objetivo
                // Validar la acción (on/off)
                let new_showmail = match action.to_lowercase().as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Invalid action. Use 'on' or 'off'.", client)).await?;
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
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The showmail option for {} has been {}.", client, nick, status)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The nick {} is not registered.", client, nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The database is not configured.", client)).await?;
                }
            }
            "password" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Usage: /NS PASSWORD <password>", client)).await?;
                    return Ok(());
                }

                let new_password = params[0];
                
                // Validar la contraseña
                if new_password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The password must be at least 6 characters long.", client)).await?;
                    return Ok(());
                }
                
                if new_password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The password cannot be more than 32 characters long.", client)).await?;
                    return Ok(());
                }
                
                // Verificar que la contraseña no contenga caracteres no permitidos
                if !new_password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The password contains invalid characters.", client)).await?;
                    return Ok(());
                }
                
                // Validar que la contraseña no esté vacía
                if new_password.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The password cannot be empty.", client)).await?;
                    return Ok(());
                }

                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Cambiar la contraseña
                    db.update_nick_password(nick, new_password).await?;
                    
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The password for {} has been changed successfully.", client, nick)).await?;
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The database is not configured.", client)).await?;
                }
                
            }
            "vhost" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Usage: /NS VHOST <off|tu.ip.virtual>", client)).await?;
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
                                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :You must wait {}h {}m before changing your vhost again.", client, hours, minutes)).await?;
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
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu vhost ha sido {}.", client, status)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The nick {} is not registered.", client, nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The database is not configured.", client)).await?;
                }
            }
            "identify" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Usage: /NS IDENTIFY <nickname> <password>", client)).await?;
                    return Ok(());
                }

                let target_nick = params[0];
                let password = params[1];
                
                // Validar la contraseña
                if password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The password must be at least 6 characters long.", client)).await?;
                    return Ok(());
                }
                
                if password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The password cannot be more than 32 characters long.", client)).await?;
                    return Ok(());
                }
                
                // Verificar que la contraseña no contenga caracteres no permitidos
                if !password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The password contains invalid characters.", client)).await?;
                    return Ok(());
                }
                
                // Validar el nickname objetivo
                if let Err(_) = validate_username(target_nick) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Invalid nick.", client)).await?;
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
                                                if nicknames != target_nick.to_string() && nicknames != old_nick.to_string() {
                                                    if let Some(user) = state.users.get_mut(&crate::state::structs::to_unicase(&nicknames)) {
                                                        let part_msg = format!("PART {} :vHost", channel);
                                                        let _ = user.send_msg_display(
                                                            &old_source,
                                                            part_msg.as_str()
                                                        );
                                                        let join_msg = format!("JOIN :{}", channel);
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
                                                                let msg = format!("MODE {} +{} {}",
                                                                    channel, mode, target_nick);
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
                                state.users.insert(crate::state::structs::to_unicase(&target_nick.to_string()), user.clone());
                            }
                            
                            // Obtener el nuevo client_name después de las modificaciones
                            let new_client = conn_state.user_state.client_name();
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :You have been successfully identified as {}.", new_client, target_nick)).await?;
                            
                            // Notificar a todos los usuarios sobre el cambio de nick
                            let nick_change_msg = format!("NICK :{}", target_nick);
                            for u in state.users.values() {
                                let _ = u.send_msg_display(&old_source, nick_change_msg.clone());
                            }
                            
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Incorrect password.", client)).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The nick {} is not registered.", client, target_nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The database is not configured.", client)).await?;
                }
            }
            "info" => {
                if let Some(db_arc) = &self.databases.nick_db {
                    let db = db_arc.read().await;
                    if let Some((user, registration_date, email, url, vhost, last_vhost, noaccess, noop, showmail)) = db.get_nick_info(nick).await? {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Nick information: {}", client, nick)).await?;
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :User: {}", client, user)).await?;
                        if let Some(vhost) = &vhost {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Vhost: {}", client, vhost)).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Vhost: Not configured", client)).await?;
                        }
                        if let Some(last_vhost) = &last_vhost {
                            if let Ok(duration) = SystemTime::now().duration_since(*last_vhost) {
                                let days = duration.as_secs() / 86400;
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Last vhost change: {} days", client, days)).await?;
                            }
                        }
                        if showmail {
                            if let Some(email) = &email {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Email: {}", client, email)).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Email: No set", client)).await?;
                            }
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Email: Not shown", client)).await?;
                        }
                        if let Some(url) = &url {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :URL: {}", client, url)).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :URL: No set", client)).await?;
                        }
                        if let Ok(duration) = SystemTime::now().duration_since(registration_date) {
                            let days = duration.as_secs() / 86400;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Registered {} days ago", client, days)).await?;
                        }
                        if noaccess {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :No access: Enabled", client)).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :No access: Disabled", client)).await?;
                        }
                        if noop { 
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :No op: Enabled", client)).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :No op: Disabled", client)).await?;
                        }
                        if showmail {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Showmail: Enabled", client)).await?;  
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Showmail: Disabled", client)).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :The nick {} is not registered.", client, nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Database is not configured.", client)).await?;
                }
            }
            "help" => {
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :NickServ commands:", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  REGISTER <password> - Register your nick", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  DROP <password> - Delete your nick registration", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  EMAIL <email|OFF> - Set or disable your email", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  URL <url|OFF> - Set or disable your URL", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  VHOST <vhost|OFF> - Set or disable your vhost", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  NOACCESS <on|off> - Enable or disable no access mode", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  NOOP <on|off> - Enable or disable no op mode", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  SHOWMAIL <on|off> - Enable or disable showmail mode", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  PASSWORD <password> - Change your password", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  IDENTIFY <nickname> <password> - Identify yourself to the server", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  INFO [nick] - Show nick information", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  HELP - Show available commands", client)).await?;
            }
            _ => {
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Unknown command. Use /NS HELP for available commands.", client)).await?;
            }
        }
        Ok(())
    }
}
