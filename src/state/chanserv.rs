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
                    if let Some(channel_info) = db.get_channel_info(channel).await? {
                        // Verificar permisos: solo el creador del canal o un IRCop pueden dropearlo
                        let is_creator = nick == &channel_info.0;
                        let is_ircop = self.is_ircop(nick).await;
                        
                        if !is_creator && !is_ircop {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :No tienes permisos para eliminar el canal '{}'. Solo el creador del canal o un IRCop pueden hacerlo.", client, channel)).await?;
                            return Ok(());
                        }
                        
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
                        
                        // Información básica del canal
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Información del canal {}: Creado por {} el {}", client, channel, info.0, datetime)).await?;
                        
                        // Mostrar topic si existe
                        if let Some(topic) = &info.2 {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Topic: {}", client, topic)).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Topic: No establecido", client)).await?;
                        }
                        
                        // Mostrar modos del canal
                        let state = self.state.read().await;
                        if let Some(chanobj) = state.channels.get(channel) {
                            let modes = chanobj.modes.to_string();
                            if !modes.is_empty() {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Modos: {}", client, modes)).await?;
                            } else {
                                self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Modos: Sin modos especiales", client)).await?;
                            }
                        }
                        drop(state);
                        
                        // Mostrar acceso del usuario si lo tiene
                        if let Ok(Some((access_level, setter, set_time))) = db.get_channel_access(channel, nick).await {
                            let access_time = set_time.duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let access_datetime = chrono::DateTime::from_timestamp(access_time as i64, 0)
                                .unwrap_or_default()
                                .format("%Y-%m-%d %H:%M:%S");
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Tu acceso: {} (establecido por {} el {})", client, access_level.to_uppercase(), setter, access_datetime)).await?;
                        }
                        
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
                    
                    // Verificar permisos: solo SOP y el creador del canal pueden cambiar accesos
                    let channel_info = db.get_channel_info(channel).await?;
                    let is_creator = nick == &channel_info.unwrap().0;
                    let is_sop = db.get_channel_access(channel, nick).await?.map_or(false, |access| access.0 == "sop");
                    let is_ircop = self.is_ircop(nick).await;
                    
                    if !is_creator && !is_sop && !is_ircop {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :No tienes permisos para modificar accesos en '{}'. Solo los SOP y el creador del canal pueden hacerlo.", client, channel)).await?;
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
            "transfer" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS TRANSFER <canal> <nick>", client)).await?;
                    return Ok(());
                }
                let channel = params[0];
                let target_nick = params[1];

                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_none() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' no está registrado.", client, channel)).await?;
                        return Ok(());
                    }
                    
                    // Verificar que el usuario objetivo esté registrado con NickServ
                    if let Some(nick_db_arc) = &self.databases.nick_db {
                        let nick_db = nick_db_arc.read().await;
                        if nick_db.get_nick_info(target_nick).await?.is_none() {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El usuario '{}' no está registrado con NickServ.", client, target_nick)).await?;
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
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :No tienes permisos para transferir el canal '{}'. Solo el propietario o un IRCop puede hacerlo.", client, channel)).await?;
                            return Ok(());
                        }
                        
                        // Realizar la transferencia - actualizar el propietario del canal
                        db.update_channel_owner(channel, target_nick).await?;
                        
                        // Notificar el cambio
                        if is_ircop {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' ha sido transferido de {} a {} por un IRCop.", client, channel, current_owner, target_nick)).await?;
                        } else {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' ha sido transferido exitosamente a {}.", client, channel, target_nick)).await?;
                        }
                        
                        // Notificar al nuevo propietario
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Has recibido la propiedad del canal '{}' de {}.", target_nick, channel, current_owner)).await?;
                        
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Error al obtener información del canal '{}'.", client, channel)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            } "topic" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS TOPIC <canal> <topic>", client)).await?;
                    return Ok(());
                }

                let channel = params[0];
                let new_topic = params[1..].join(" ");

                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_none() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' no está registrado.", client, channel)).await?;
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
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :No tienes permisos para cambiar el topic del canal '{}'. Necesitas ser IRCop, propietario, AOP o superior.", client, channel)).await?;
                            return Ok(());
                        }
                        
                        // Cambiar el topic del canal
                        db.update_channel_info(channel, Some(&new_topic), None).await?;
                        
                        // Notificar el cambio
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El topic del canal '{}' ha sido cambiado a: {}", client, channel, new_topic)).await?;
                        
                        // Enviar el cambio de topic a todos los usuarios del canal
                        let mut state = self.state.write().await;
                        if let Some(chanobj) = state.channels.get_mut(channel) {
                            // Actualizar el topic en la lógica interna del IRCd
                            chanobj.topic = Some(ChannelTopic::new_with_nick(new_topic.clone(), nick.to_string()));
                            
                            let topic_msg = format!("TOPIC {} :{}", channel, new_topic);
                            // Recolectar los nicks de usuarios para evitar el conflicto de préstamo
                            let user_nicks: Vec<String> = chanobj.users.keys().cloned().collect();
                            drop(state); // Liberar el préstamo mutable
                            
                            // Ahora acceder a los usuarios por separado
                            let state = self.state.read().await;
                            for user_nick in user_nicks {
                                if let Some(user) = state.users.get(&user_nick) {
                                    // Enviar el mensaje usando el sender del usuario
                                    let _ = user.send_msg_display("ChanServ", &topic_msg);
                                }
                            }
                        }
                        
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Error al obtener información del canal '{}'.", client, channel)).await?;
                    }
                } else {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :La base de datos no está configurada.", client)).await?;
                }
            } "mlock" => {
                if params.len() < 2 {
                    self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Uso: /CS MLOCK <canal> [+modos|-modos]", client)).await?;
                    return Ok(());
                }
                let channel = params[0];
                let modes = params[1];
                
                // Verificar que el canal existe
                if let Some(db_arc) = &self.databases.chan_db {
                    let mut db = db_arc.write().await;
                    if db.get_channel_info(channel).await?.is_none() {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :El canal '{}' no está registrado.", client, channel)).await?;
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
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :No tienes permisos para modificar el mlock del canal '{}'. Necesitas ser IRCop, propietario, AOP o superior.", client, channel)).await?;
                            return Ok(());
                        }
                        
                        // Validar que los modos son válidos para mlock
                        if !self.validate_mlock_modes(modes) {
                            self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Modos inválidos para mlock. Modos permitidos: +ntklmi", client)).await?;
                            return Ok(());
                        }
                        
                        // Actualizar el mlock en la base de datos
                        db.update_channel_info(channel, None, Some(modes)).await?;
                        
                        // Notificar el cambio
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :MLock del canal '{}' establecido a: {}", client, channel, modes)).await?;
                        
                    } else {
                        self.feed_msg_source(&mut conn_state.stream, "ChanServ", format!("NOTICE {} :Error al obtener información del canal '{}'.", client, channel)).await?;
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

    /// Valida que los modos proporcionados son válidos para mlock
    fn validate_mlock_modes(&self, modes: &str) -> bool {
        if modes.is_empty() {
            return true; // Permitir mlock vacío
        }
        
        // Verificar que comience con + o -
        if !modes.starts_with('+') && !modes.starts_with('-') {
            return false;
        }
        
        // Modos permitidos para mlock: n, t, k, l, m, i
        let allowed_modes = ['n', 't', 'k', 'l', 'm', 'i'];
        
        // Verificar cada carácter después del signo
        for c in modes[1..].chars() {
            if !allowed_modes.contains(&c) {
                return false;
            }
        }
        
        true
    }
}


