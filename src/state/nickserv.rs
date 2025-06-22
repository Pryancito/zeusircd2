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
                
                // Validar la contraseña
                if password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña debe tener al menos 6 caracteres.", client)).await?;
                    return Ok(());
                }
                
                if password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña no puede tener más de 32 caracteres.", client)).await?;
                    return Ok(());
                }
                
                // Verificar que la contraseña no contenga caracteres no permitidos
                if !password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña contiene caracteres no permitidos.", client)).await?;
                    return Ok(());
                }
                
                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    if db.get_nick_info(nick).await?.is_some() {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick '{}' ya está registrado.", client, nick)).await?;
                        return Ok(());
                    }

                    let password_hash = argon2_hash_password(password);

                    db.add_nick(nick, &password_hash, &conn_state.user_state.source, SystemTime::now(), None, None, None, None, false, false, false).await?;
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
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS EMAIL <email> o /NS EMAIL OFF", client)).await?;
                    return Ok(());
                }

                let email = params[0];
                
                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        if email.to_lowercase() == "off" {
                            // Desactivar email
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), None, info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu email ha sido desactivado.", client)).await?;
                        } else {
                            // Validar formato de email
                            if !email.contains('@') || !email.contains('.') {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Formato de email inválido.", client)).await?;
                                return Ok(());
                            }
                            
                            // Actualizar el email
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), Some(email), info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu email ha sido actualizado a: {}", client, email)).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu nick no está registrado.", client)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "url" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS URL <url> o /NS URL OFF", client)).await?;
                    return Ok(());
                }

                let url = params[0];
                
                if let Some(db_arc) = &self.databases.nick_db {
                    let mut db = db_arc.write().await;
                    
                    // Verificar si el nick está registrado
                    if let Some(info) = db.get_nick_info(nick).await? {
                        if url.to_lowercase() == "off" {
                            // Desactivar URL
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), None, info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu URL ha sido desactivada.", client)).await?;
                        } else {
                            // Validar formato de URL básico
                            if !url.starts_with("http://") && !url.starts_with("https://") {
                                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La URL debe comenzar con http:// o https://", client)).await?;
                                return Ok(());
                            }
                            
                            // Actualizar la URL
                            db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), Some(url), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(info.8)).await?;
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Tu URL ha sido actualizada a: {}", client, url)).await?;
                        }
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
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), info.5, Some(new_noaccess), Some(info.6), Some(info.7)).await?;
                        
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
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS NOOP <nick> <on|off>", client)).await?;
                    return Ok(());
                }

                let target_nick = params[0];
                let action = params[1];
                
                // Validar el nick objetivo
                if let Err(_) = validate_username(target_nick) {
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
                    if let Some(info) = db.get_nick_info(target_nick).await? {
                        // Actualizar el estado de noop
                        db.update_nick_info(target_nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(new_noop), Some(info.7)).await?;
                        
                        let status = if new_noop { "activada" } else { "desactivada" };
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La opción noop para {} ha sido {}.", client, target_nick, status)).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick {} no está registrado.", client, target_nick)).await?;
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
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), info.3.as_deref(), info.4.as_deref(), info.5, Some(info.6), Some(info.7), Some(new_showmail)).await?;
                        
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
                
                // Validar la contraseña
                if new_password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña debe tener al menos 6 caracteres.", client)).await?;
                    return Ok(());
                }
                
                if new_password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña no puede tener más de 32 caracteres.", client)).await?;
                    return Ok(());
                }
                
                // Verificar que la contraseña no contenga caracteres no permitidos
                if !new_password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña contiene caracteres no permitidos.", client)).await?;
                    return Ok(());
                }
                
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
            "vhost" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS VHOST <off|tu.ip.virtual>", client)).await?;
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
                                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Debes esperar {}h {}m antes de cambiar tu vhost nuevamente.", client, hours, minutes)).await?;
                                    return Ok(());
                                }
                            }
                        }
                        
                        // Actualizar el vhost y el timestamp
                        let now = SystemTime::now();
                        db.update_nick_info(nick, Some(conn_state.user_state.source.as_str()), Some(info.1), info.2.as_deref(), info.3.as_deref(), new_vhost.as_deref(), Some(now), Some(info.6), Some(info.7), Some(info.8)).await?;
                        
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
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick {} no está registrado.", client, nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "identify" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Uso: /NS IDENTIFY <nickname> <password>", client)).await?;
                    return Ok(());
                }

                let target_nick = params[0];
                let password = params[1];
                
                // Validar la contraseña
                if password.len() < 6 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña debe tener al menos 6 caracteres.", client)).await?;
                    return Ok(());
                }
                
                if password.len() > 32 {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña no puede tener más de 32 caracteres.", client)).await?;
                    return Ok(());
                }
                
                // Verificar que la contraseña no contenga caracteres no permitidos
                if !password.chars().all(|c| c.is_ascii_alphanumeric() || c.is_ascii_punctuation()) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La contraseña contiene caracteres no permitidos.", client)).await?;
                    return Ok(());
                }
                
                // Validar el nickname objetivo
                if let Err(_) = validate_username(target_nick) {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Nick inválido.", client)).await?;
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
                                if state.users.contains_key(target_nick) {
                                    // El nick está en uso, preparar para desconectar
                                    Some(target_nick.to_string())
                                } else {
                                    None
                                }
                            };
                            
                            // Desconectar al usuario existente si es necesario
                            if let Some(existing_nick) = user_to_disconnect {
                                let mut state = self.state.write().await;
                                if let Some(user) = state.users.remove(&existing_nick) {
                                    // Enviar mensaje de desconexión al usuario existente
                                    if let Some(sender) = user.quit_sender {
                                        let _ = sender.send((existing_nick.clone(), "NickServ: Nick reclamado".to_string()));
                                    }
                                }
                            }
                            
                            // Cambiar el nick del usuario actual
                            let old_nick = nick.clone();
                            let old_source = conn_state.user_state.source.clone();
                            
                            // Actualizar el nick en el estado del usuario
                            conn_state.user_state.set_nick(target_nick.to_string());
                            conn_state.user_state.password = Some(password.to_string());

                            if vhost.is_some() {
                                conn_state.user_state.set_cloack(vhost.clone().expect("ERROR.in.vHost"));
                            }
                            
                            // Actualizar en el estado global
                            let mut state = self.state.write().await;
                            if let Some(mut user) = state.users.remove(&old_nick) {
                                user.update_nick(&conn_state.user_state);
                                
                                // Actualizar canales
                                for ch in &user.channels {
                                    if let Some(channel) = state.channels.get_mut(&ch.clone()) {
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
                                state.users.insert(target_nick.to_string(), user);
                            }
                            
                            // Obtener el nuevo client_name después de las modificaciones
                            let new_client = conn_state.user_state.client_name();
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Te has identificado exitosamente como {}.", new_client, target_nick)).await?;
                            
                            // Notificar a todos los usuarios sobre el cambio de nick
                            let nick_change_msg = format!(":{} NICK :{}", old_source, target_nick);
                            for u in state.users.values() {
                                let _ = u.send_msg_display(&old_source, nick_change_msg.clone());
                            }
                            
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Contraseña incorrecta.", client)).await?;
                        }
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :El nick {} no está registrado.", client, target_nick)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            }
            "help" => {
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Comandos disponibles:", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  REGISTER <password> - Registrar tu nick", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  IDENTIFY <nickname> <password> - Identificarte con un nick registrado", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  DROP <password> - Eliminar tu nick", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  PASSWORD <new_password> - Cambiar contraseña", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  EMAIL <email> - Configurar email", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  URL <url> - Configurar URL", client)).await?;
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :  VHOST <vhost> - Configurar vhost", client)).await?;
            }
            _ => {
                self.feed_msg_source(&mut conn_state.stream, "NickServ", format!("NOTICE {} :Comando desconocido. Usa /NS HELP.", client)).await?;
            }
        }
        Ok(())
    }
}
