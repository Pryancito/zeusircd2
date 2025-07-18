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
            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You don't have a nick.")).await?;
            return Ok(());
        };

        match subcommand.to_lowercase().as_str() {
            "register" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Usage: /CS REGISTER <channel>")).await?;
                    return Ok(());
                }
                let channel = params[0];

                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_some() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Channel '{channel}' is already registered.")).await?;
                        return Ok(());
                    }

                    db.add_channel(channel, nick, SystemTime::now()).await?;
                    
                    // Establecer automáticamente el modo +r para canales registrados
                    let mut state = self.state.write().await;
                    if let Some(chanobj) = state.channels.get_mut(&crate::state::structs::to_unicase(channel)) {
                        chanobj.modes.registered = true;
                        
                        // Notificar a todos los usuarios del canal sobre el cambio de modo
                        let nicks: Vec<String> = chanobj.users.keys().cloned().map(|nick| nick.to_string()).collect();
                        for nick in nicks {
                            if let Some(user) = state.users.get_mut(&crate::state::structs::to_unicase(&nick)) {
                                let mensaje = format!("MODE {} +r", channel);
                                let _ = user.send_msg_display(&self.config.name, &mensaje);
                            }
                        }
                    }
                    
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Channel '{channel}' has been registered.")).await?;
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "drop" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Usage: /CS DROP <channel>")).await?;
                    return Ok(());
                }
                let channel = params[0];

                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if let Some(channel_info) = db.get_channel_info(channel).await? {
                        // Check permissions: only the channel creator or an IRCop can drop it
                        let is_creator = nick == &channel_info.0;
                        let is_ircop = self.is_ircop(nick).await;
                        
                        if !is_creator && !is_ircop {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You don't have permission to delete channel '{channel}'. Only the channel creator or an IRCop can do this.")).await?;
                            return Ok(());
                        }
                        
                        db.delete_channel(channel).await?;
                        
                        // Quitar automáticamente el modo +r cuando se elimina el canal
                        let mut state = self.state.write().await;
                        if let Some(chanobj) = state.channels.get_mut(&crate::state::structs::to_unicase(channel)) {
                            if chanobj.modes.registered {
                                chanobj.modes.registered = false;
                                
                                // Notificar a todos los usuarios del canal sobre el cambio de modo
                                let nicks: Vec<String> = chanobj.users.keys().cloned().map(|nick| nick.to_string()).collect();
                                for nick in nicks {
                                    if let Some(user) = state.users.get_mut(&crate::state::structs::to_unicase(&nick)) {
                                        let mensaje = format!("MODE {} -r", channel);
                                        let _ = user.send_msg_display(&self.config.name, &mensaje);
                                    }
                                }
                            }
                        }
                        
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Channel '{channel}' has been deleted.")).await?;
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Channel '{channel}' is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "info" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Usage: /CS INFO <channel>")).await?;
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
                        
                        // Basic channel information
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Channel {channel} information: Created by {} on {datetime}", info.0)).await?;
                        
                        // Show topic if it exists
                        if let Some(topic) = &info.2 {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Topic: {topic}")).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Topic: Not set")).await?;
                        }
                        
                        // Show topic setter and time if available
                        if let Some(topic_setter) = &info.4 {
                            if let Some(topic_time) = &info.5 {
                                let topic_datetime = chrono::DateTime::from_timestamp(topic_time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs() as i64, 0)
                                    .unwrap_or_default()
                                    .format("%Y-%m-%d %H:%M:%S");
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Topic set by {topic_setter} on {topic_datetime}")).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Topic set by {topic_setter}")).await?;
                            }
                        }
                        
                        // Show channel modes from database
                        let modos_str = match &info.3 {
                            Some(modos) if !modos.is_empty() => modos.as_str(),
                            _ => "No special modes",
                        };
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Modes: {modos_str}")).await?;
                        
                        // Show user access if they have any
                        if let Ok(Some((access_level, setter, set_time))) = db.get_channel_access(channel, nick).await {
                            let access_time = set_time.duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let access_datetime = chrono::DateTime::from_timestamp(access_time as i64, 0)
                                .unwrap_or_default()
                                .format("%Y-%m-%d %H:%M:%S");
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Your access: {access_level} (set by {setter} on {access_datetime})")).await?;
                        }
                        
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Channel '{channel}' is not registered.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "vop" | "hop" | "aop" | "sop" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Usage: /CS {subcommand} <channel> <add|del|list> [nick]")).await?;
                    return Ok(());
                }
                let channel = params[0];
                let action = params[1].to_lowercase();
                
                // Check if channel exists
                if let Some(db_arc) = &self.databases.chan_db {
                    let db = db_arc.read().await;
                    if db.get_channel_info(channel).await?.is_none() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Channel '{channel}' is not registered.")).await?;
                        return Ok(());
                    }
                    
                    // Check permissions: only SOP and channel creator can change access
                    let channel_info = db.get_channel_info(channel).await?;
                    let is_creator = nick == &channel_info.unwrap().0;
                    let is_sop = db.get_channel_access(channel, nick).await?.map_or(false, |access| access.0 == "sop");
                    let is_ircop = self.is_ircop(nick).await;
                    
                    if !is_creator && !is_sop && !is_ircop {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You don't have permission to modify access in '{channel}'. Only SOP and channel creator can do this.")).await?;
                        return Ok(());
                    }
                    
                    drop(db);
                    
                    let mut db = db_arc.write().await;
                    
                    match action.as_str() {
                        "add" => {
                            if params.len() < 3 {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Usage: /CS {subcommand} <channel> add <nick>")).await?;
                                return Ok(());
                            }
                            let target_nick = params[2];

                            // Solo permitir añadir usuarios registrados con NickServ
                            let mut registrado = false;
                            if let Some(nick_db_arc) = &self.databases.nick_db {
                                let nick_db = nick_db_arc.read().await;
                                if let Ok(Some((_user, _registration_date, _email, _url, _vhost, _last_vhost, noaccess, _noop, _showmail))) = nick_db.get_nick_info(target_nick).await {
                                    if noaccess {
                                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Cannot add {target_nick} to access list. User has noaccess mode enabled.")).await?;
                                        return Ok(());
                                    }
                                    registrado = true;
                                }
                            }
                            if !registrado {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You can only add users registered with NickServ to the access list.")).await?;
                                return Ok(());
                            }

                            // Check if access already exists
                            if let Some(existing) = db.get_channel_access(channel, target_nick).await? {
                                if existing.0 == subcommand.to_lowercase() {
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :{target_nick} already has {subcommand} access in {channel}.")).await?;
                                } else {
                                    // Update access level
                                    db.update_channel_access(channel, target_nick, &subcommand.to_lowercase(), nick, SystemTime::now()).await?;
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :{target_nick}'s access level in {channel} has been updated to {subcommand}")).await?;
                                }
                            } else {
                                // Add new access
                                db.add_channel_access(channel, target_nick, &subcommand.to_lowercase(), nick, SystemTime::now()).await?;
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :{target_nick} has been added to the {subcommand} list of {channel}.")).await?;
                            }
                        }
                        "del" => {
                            if params.len() < 3 {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Usage: /CS {subcommand} <channel> del <nick>")).await?;
                                return Ok(());
                            }
                            let target_nick = params[2];
                            
                            // Check if access exists
                            if let Some(existing) = db.get_channel_access(channel, target_nick).await? {
                                if existing.0 == subcommand.to_lowercase() {
                                    db.delete_channel_access(channel, target_nick).await?;
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :{target_nick} has been removed from the {subcommand} list of {channel}.")).await?;
                                } else {
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :{target_nick} doesn't have {subcommand} access in {channel}.")).await?;
                                }
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :{target_nick} doesn't have any access in {channel}.")).await?;
                            }
                        }
                        "list" => {
                            // List all users with this access level
                            let access_list = db.get_channel_access_list(channel, Some(&subcommand.to_lowercase())).await?;
                            if access_list.is_empty() {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :No users with {subcommand} access in {channel}.")).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :List {subcommand} of {channel}:")).await?;
                                for (nick, _level, added_by, added_time) in access_list {
                                    let datetime = chrono::DateTime::from_timestamp(added_time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs() as i64, 0)
                                        .unwrap_or_default()
                                        .format("%Y-%m-%d %H:%M:%S");
                                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  {nick} (added by {added_by} on {datetime})")).await?;
                                }
                            }
                        }
                        _ => {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Invalid action. Use: add, del, or list")).await?;
                        }
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Database is not configured.")).await?;
                }
            }
            "transfer" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Uso: /CS TRANSFER <canal> <nick>")).await?;
                    return Ok(());
                }
                let channel = params[0];
                let target_nick = params[1];

                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_none() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :El canal '{channel}' no está registrado.")).await?;
                        return Ok(());
                    }
                    
                    // Verificar que el usuario objetivo esté registrado con NickServ
                    if let Some(nick_db_arc) = &self.databases.nick_db {
                        let nick_db = nick_db_arc.read().await;
                        if nick_db.get_nick_info(target_nick).await?.is_none() {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :El usuario '{target_nick}' no está registrado con NickServ.")).await?;
                            return Ok(());
                        }
                    }
                    
                    // Obtener información del canal para verificar permisos
                    if let Some(channel_info) = db.get_channel_info(channel).await? {
                        let current_owner = &channel_info.0; // El primer elemento es el propietario
                        
                        // Verificar permisos: solo el propietario del canal o un IRCop puede transferir
                        let is_owner = nick == current_owner;
                        let is_ircop = self.is_ircop(nick).await;
                        
                        if !is_owner && !is_ircop {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :No tienes permisos para transferir el canal '{channel}'. Solo el propietario o un IRCop puede hacerlo.")).await?;
                            return Ok(());
                        }
                        
                        // Realizar la transferencia - actualizar el propietario del canal
                        db.update_channel_owner(channel, target_nick).await?;
                        
                        // Notificar el cambio
                        if is_ircop {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :El canal '{channel}' ha sido transferido de {current_owner} a {target_nick} por un IRCop.")).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :El canal '{channel}' ha sido transferido exitosamente a {target_nick}.")).await?;
                        }
                        
                        // Notificar al nuevo propietario
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Has recibido la propiedad del canal '{channel}' de {current_owner}.")).await?;
                        
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Error al obtener información del canal '{channel}'.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :La base de datos no está configurada.")).await?;
                }
            } "topic" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Usage: /CS TOPIC <channel> <topic>")).await?;
                    return Ok(());
                }

                let channel = params[0];
                let new_topic = params[1..].join(" ");

                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_none() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :The channel '{channel}' is not registered.")).await?;
                        return Ok(());
                    }

                    // Obtener información del canal para verificar permisos
                    if let Some(channel_info) = db.get_channel_info(channel).await? {
                        let current_owner = &channel_info.0;
                        
                        // Verificar permisos: IRCop, propietario del canal, o acceso AOP o superior
                        let is_owner = nick == current_owner;
                        let is_ircop = self.is_ircop(nick).await;
                        let has_access = if let Ok(Some((access_level, _, _))) = db.get_channel_access(channel, nick).await {
                            matches!(access_level.as_str(), "aop" | "sop")
                        } else {
                            false
                        };
                        
                        if !is_owner && !is_ircop && !has_access {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You don't have permission to change the topic of the channel '{channel}'. You need to be an IRCop, owner, AOP or superior.")).await?;
                            return Ok(());
                        }
                        
                        // Change channel topic
                        db.update_channel_info(channel, Some(&new_topic), Some(nick), Some(SystemTime::now()), None).await?;
                        
                        // Notify the change
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :The topic of the channel '{channel}' has been changed to: {new_topic}")).await?;
                        
                        // Send topic change to all users in the channel
                        let mut state = self.state.write().await;
                        if let Some(chanobj) = state.channels.get_mut(&crate::state::structs::to_unicase(channel)) {
                            // Update topic in internal IRCd logic
                            chanobj.topic = Some(ChannelTopic::new_with_nick(new_topic.clone(), nick.to_string()));
                            
                            let topic_msg = format!("TOPIC {} :{}", channel, new_topic);
                            // Collect user nicks to avoid borrowing conflict
                            let user_nicks: Vec<String> = chanobj.users.keys().cloned().map(|nick| nick.to_string()).collect();
                            drop(state); // Release mutable borrow
                            
                            // Now access users separately
                            let state = self.state.read().await;
                            for user_nick in user_nicks {
                                if let Some(user) = state.users.get(&crate::state::structs::to_unicase(&user_nick)) {
                                    // Send message using user's sender
                                    let _ = user.send_msg_display("ChanServ", &topic_msg);
                                }
                            }
                        }
                        
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Error getting information about the channel '{channel}'.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :The database is not configured.")).await?;
                }
            } "mlock" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Usage: /CS MLOCK <channel> [+modes|-modes|off] [arguments]")).await?;
                    return Ok(());
                }
                let channel = params[0];
                let args = params[1..].join(" ");
                let modo_str = args.split_whitespace().next().unwrap_or("");
                if modo_str.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You must specify the modes for MLOCK or 'off' to deactivate it.")).await?;
                    return Ok(());
                }

                // Permitir desactivar el mlock con "off"
                let desactivar = modo_str.eq_ignore_ascii_case("off");

                if !desactivar && !self.validate_mlock_modes(modo_str) {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Invalid modes for mlock. Allowed modes: +ntklmirO. The +O mode restricts the channel only to IRCops. The +r mode marks the channel as registered.")).await?;
                    return Ok(());
                }

                // Añadir a los modos permitidos el modo +r si no está presente, tanto en modo_str como en args
                let (modo_str, args) = if !modo_str.contains('r') {
                    let nuevo_modo_str = format!("+r{}", &modo_str[1..]);
                    let mut args_split = args.split_whitespace();
                    let _ = args_split.next(); // saltar el primer argumento (modo_str original)
                    let resto_args: Vec<&str> = args_split.collect();
                    let nuevo_args = if resto_args.is_empty() {
                        nuevo_modo_str.clone()
                    } else {
                        format!("{} {}", nuevo_modo_str, resto_args.join(" "))
                    };
                    (nuevo_modo_str, nuevo_args)
                } else {
                    (modo_str.to_string(), args.clone())
                };

                // Verificar que el canal existe
                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_none() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :The channel '{channel}' is not registered.")).await?;
                        return Ok(());
                    }
                    
                    // Obtener información del canal para verificar permisos
                    if let Some(channel_info) = db.get_channel_info(channel).await? {
                        let current_owner = &channel_info.0;
                        
                        // Verificar permisos: IRCop, propietario del canal, o acceso AOP o superior
                        let is_owner = nick == current_owner;
                        let is_ircop = self.is_ircop(nick).await;
                        let has_access = if let Ok(Some((access_level, _, _))) = db.get_channel_access(channel, nick).await {
                            matches!(access_level.as_str(), "aop" | "sop")
                        } else {
                            false
                        };
                        
                        if !is_owner && !is_ircop && !has_access {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You don't have permission to modify the mlock of the channel '{channel}'. You need to be an IRCop, owner, AOP or superior.")).await?;
                            return Ok(());
                        }
                        
                        // Si se solicita desactivar el mlock
                        if desactivar {
                            db.update_channel_info(channel, None, None, None, Some("")).await?;
                            // Limpiar modos mlock en la lógica interna si el canal existe
                            let mut state = self.state.write().await;
                            if let Some(chanobj) = state.channels.get_mut(&crate::state::structs::to_unicase(channel)) {
                                // Limpiar los modos mlock (solo los modos permitidos por mlock)
                                self.apply_stored_modes(&mut chanobj.modes, "");
                            }
                            drop(state);
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :MLock of the channel '{channel}' has been deactivated.")).await?;
                        } else {
                            // Actualizar el mlock en la base de datos
                            db.update_channel_info(channel, None, None, None, Some(&modo_str)).await?;
                            
                            // Aplicar los modos al canal si existe
                            let mut state = self.state.write().await;
                            if let Some(chanobj) = state.channels.get_mut(&crate::state::structs::to_unicase(channel)) {
                                // Limpiar modos anteriores y aplicar los nuevos
                                self.apply_stored_modes(&mut chanobj.modes, &args);
                            }
                            drop(state);
                            
                            // Notificar el cambio
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :MLock of the channel '{channel}' has been set to: {modo_str}")).await?;
                        }
                        
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Error getting information about the channel '{channel}'.")).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :The database is not configured.")).await?;
                }
            }
            "help" => {
                if params.is_empty() {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :ChanServ - Channel Registration Service")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Available commands:")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  REGISTER <channel> <description> - Register a channel")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  DROP <channel> - Delete a channel registration")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  INFO <channel> - Show channel information")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  TOPIC <channel> <topic> - Set channel topic")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  MLOCK <channel> <modes> - Set channel mode lock")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  VOP <channel> <add|del|list> [nick] - Manage voice operators")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  HOP <channel> <add|del|list> [nick] - Manage half operators")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  AOP <channel> <add|del|list> [nick] - Manage auto operators")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  SOP <channel> <add|del|list> [nick] - Manage super operators")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  HELP <command> - Get detailed help for a command")).await?;
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :  TRANSFER <channel> <nick> - Transfer channel ownership")).await?;
                    return Ok(());
                }
                
                let command = params[0].to_lowercase();
                match command.as_str() {
                    "register" => {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :REGISTER <channel> <description>")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Registers a channel with ChanServ.")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You must be the channel founder to register it.")).await?;
                    }
                    "drop" => {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :DROP <channel>")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Deletes a channel registration.")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Only the channel founder or an IRCop can drop a channel.")).await?;
                    }
                    "info" => {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :INFO <channel>")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Shows detailed information about a registered channel.")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Displays founder, creation date, topic, modes, and your access level.")).await?;
                    }
                    "topic" => {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :TOPIC <channel> <topic>")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Sets the topic for a registered channel.")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You must be the channel founder or have AOP+ access.")).await?;
                    }
                    "mlock" => {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :MLOCK <channel> <modes>")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Sets mode lock for a channel. Use 'OFF' to disable.")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Allowed modes: n, t, k, l, m, i, O")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :You must be the channel founder or have AOP+ access.")).await?;
                    }
                    "vop" | "hop" | "aop" | "sop" => {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :{} <channel> <add|del|list> [nick]", command.to_uppercase())).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Manages {} access for a channel.", command.to_uppercase())).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Access levels: VOP < HOP < AOP < SOP")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Only SOP and channel founder can modify access.")).await?;
                    }
                    "transfer" => {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :TRANSFER <channel> <nick>")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Transfers channel ownership to another user.")).await?;
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Only the channel founder or an IRCop can transfer ownership.")).await?;
                    }
                    _ => {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Unknown command '{}'. Use /CS HELP for available commands.", command)).await?;
                    }
                }
            }
            _ => {
                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {client} :Unknown command. Use /CS HELP for available commands.")).await?;
            }
        }
        Ok(())
    }

    fn validate_mlock_modes(&self, modes: &str) -> bool {
        if modes.is_empty() {
            return true; // Permitir mlock vacío
        }
        
        // Verificar que comience con + o -
        if !modes.starts_with('+') && !modes.starts_with('-') {
            return false;
        }
        
        // Modos permitidos para mlock: n, t, k, l, m, i, O
        let allowed_modes = ['n', 't', 'k', 'l', 'm', 'i', 'O', 'r'];
        
        // Verificar cada carácter después del signo
        for c in modes[1..].chars() {
            if !allowed_modes.contains(&c) {
                return false;
            }
        }
        
        true
    }

    pub(super) fn apply_stored_modes(&self, channel_modes: &mut crate::config::ChannelModes, modes_str: &str) {
        // Parsear los modos almacenados y aplicarlos al canal
        // Los modos se almacenan como string (ej: "+ntk clave123")
        let mut chars = modes_str.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '+' => {
                    // Procesar modos positivos
                    while let Some(&mode) = chars.peek() {
                        match mode {
                            'i' => {
                                channel_modes.invite_only = true;
                                chars.next();
                            }
                            'm' => {
                                channel_modes.moderated = true;
                                chars.next();
                            }
                            's' => {
                                channel_modes.secret = true;
                                chars.next();
                            }
                            't' => {
                                channel_modes.protected_topic = true;
                                chars.next();
                            }
                            'n' => {
                                channel_modes.no_external_messages = true;
                                chars.next();
                            }
                            'k' => {
                                chars.next(); // Consumir 'k'
                                // Buscar la clave después del espacio
                                let mut key = String::new();
                                while let Some(&next_ch) = chars.peek() {
                                    if next_ch.is_whitespace() {
                                        chars.next();
                                        break;
                                    }
                                    key.push(chars.next().unwrap());
                                }
                                if !key.is_empty() {
                                    channel_modes.key = Some(key);
                                }
                            }
                            'l' => {
                                chars.next(); // Consumir 'l'
                                // Buscar el límite después del espacio
                                let mut limit_str = String::new();
                                while let Some(&next_ch) = chars.peek() {
                                    if next_ch.is_whitespace() {
                                        chars.next();
                                        break;
                                    }
                                    limit_str.push(chars.next().unwrap());
                                }
                                if let Ok(limit) = limit_str.parse::<usize>() {
                                    channel_modes.client_limit = Some(limit);
                                }
                            }
                            'O' => {
                                channel_modes.only_ircops = true;
                                chars.next();
                            }
                            'r' => {
                                channel_modes.registered = true;
                                chars.next();
                            }
                            _ => {
                                // Modo desconocido, saltarlo
                                chars.next();
                            }
                        }
                    }
                }
                '-' => {
                    // Procesar modos negativos
                    while let Some(&mode) = chars.peek() {
                        match mode {
                            'i' => {
                                channel_modes.invite_only = false;
                                chars.next();
                            }
                            'm' => {
                                channel_modes.moderated = false;
                                chars.next();
                            }
                            's' => {
                                channel_modes.secret = false;
                                chars.next();
                            }
                            't' => {
                                channel_modes.protected_topic = false;
                                chars.next();
                            }
                            'n' => {
                                channel_modes.no_external_messages = false;
                                chars.next();
                            }
                            'k' => {
                                channel_modes.key = None;
                                chars.next();
                            }
                            'l' => {
                                channel_modes.client_limit = None;
                                chars.next();
                            }
                            'O' => {
                                channel_modes.only_ircops = false;
                                chars.next();
                            }
                            'r' => {
                                channel_modes.registered = false;
                                chars.next();
                            }
                            _ => {
                                // Modo desconocido, saltarlo
                                chars.next();
                            }
                        }
                    }
                }
                _ => {
                    // Carácter no reconocido, continuar
                }
            }
        }
        
    }
}


