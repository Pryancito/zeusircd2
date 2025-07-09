// mod.rs - main state
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

use chrono::prelude::*;
use futures::future::Fuse;
use futures::FutureExt;
#[cfg(feature = "dns_lookup")]
use lazy_static::lazy_static;
#[cfg(feature = "tls")]
use openssl::ssl::{Ssl, SslAcceptor, SslFiletype, SslMethod};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, RwLock};
use tokio::task::JoinHandle;
#[cfg(feature = "tls")]
use tokio_openssl::SslStream;
use tokio_stream::StreamExt;
use tokio_util::codec::LinesCodecError;
use tracing::*;
#[cfg(feature = "dns_lookup")]
use trust_dns_resolver::{TokioAsyncResolver, TokioHandle};
#[cfg(feature = "sqlite")]
use crate::database::sqlite::{SQLiteNickDatabase, SQLiteChannelDatabase};
#[cfg(feature = "mysql")]
use crate::database::mysql::mysql_impl::{MysqlNickDatabase, MysqlChannelDatabase};
#[cfg(any(feature = "sqlite", feature = "mysql"))]
use crate::database::{NickDatabase, ChannelDatabase};
use serde::ser::StdError;
use tokio::time::{timeout, Duration};

use crate::command::*;
use crate::config::*;
use crate::reply::*;
use crate::utils::*;

#[cfg(feature = "amqp")]
use crate::state::server_communication::ServerCommunication;
use Reply::*;

mod structs;
pub(crate) use structs::*;

#[cfg(any(feature = "sqlite", feature = "mysql"))]
pub(crate) struct Databases {
    pub(crate) nick_db: Option<Arc<RwLock<Box<dyn NickDatabase>>>>,
    pub(crate) chan_db: Option<Arc<RwLock<Box<dyn ChannelDatabase>>>>,
}

pub(crate) struct MainState {
    config: MainConfig,
    // key is user name
    user_config_idxs: HashMap<String, usize>,
    // key is oper name
    oper_config_idxs: HashMap<String, usize>,
    conns_count: Arc<AtomicUsize>,
    // Track connections per IP
    connections_per_ip: Arc<RwLock<HashMap<IpAddr, usize>>>,
    state: Arc<RwLock<VolatileState>>,
    #[cfg(any(feature = "sqlite", feature = "mysql"))]
    databases: Databases,
    #[cfg(feature = "amqp")]
    serv_comm: RwLock<ServerCommunication>,
    created: String,
    created_time: DateTime<Local>,
    #[cfg(any(feature = "sqlite", feature = "mysql"))]
    command_counts: [AtomicU64; 57],
    #[cfg(not(any(feature = "sqlite", feature = "mysql")))]
    command_counts: [AtomicU64; 42],
}

impl MainState {
    pub(crate) async fn new_from_config(config: MainConfig) -> Result<MainState, String> {
        // create indexes for configured users and operators.
        let mut user_config_idxs = HashMap::new();
        if let Some(ref users) = config.users {
            users.iter().enumerate().for_each(|(i, u)| {
                user_config_idxs.insert(u.name.clone(), i);
            });
        }
        let mut oper_config_idxs = HashMap::new();
        if let Some(ref opers) = config.operators {
            opers.iter().enumerate().for_each(|(i, o)| {
                oper_config_idxs.insert(o.name.clone(), i);
            });
        }
        let state = Arc::new(RwLock::new(VolatileState::new_from_config(&config)));
        #[cfg(any(feature = "sqlite", feature = "mysql"))]
        let databases = if let Some(db_config) = &config.database {
            let (mut nick_db, mut chan_db): (Box<dyn NickDatabase>, Box<dyn ChannelDatabase>) =
                match db_config.database.as_str() {
                    #[cfg(feature = "sqlite")]
                    "sqlite" => (
                        Box::new(SQLiteNickDatabase::new(&db_config.url).expect("Failed to open nick database")),
                        Box::new(SQLiteChannelDatabase::new(&db_config.url).expect("Failed to open channel database")),
                    ),
                    #[cfg(feature = "mysql")]
                    "mysql" => (
                        Box::new(MysqlNickDatabase::new()),
                        Box::new(MysqlChannelDatabase::new()),
                    ),
                    _ => return Err("Unsupported database type".to_string()),
                };

            nick_db.connect(&db_config.url).await.map_err(|e| e.to_string())?;
            nick_db.create_table().await.map_err(|e| e.to_string())?;

            chan_db.connect(&db_config.url).await.map_err(|e| e.to_string())?;
            chan_db.create_table().await.map_err(|e| e.to_string())?;
            chan_db.create_access_table().await.map_err(|e| e.to_string())?;
            
            // Run migrations on concrete instance
            chan_db.migrate_topic_fields().await.map_err(|e| e.to_string())?;

            Databases {
                nick_db: Some(Arc::new(RwLock::new(nick_db))),
                chan_db: Some(Arc::new(RwLock::new(chan_db))),
            }
        } else {
            Databases {
                nick_db: None,
                chan_db: None,
            }
        };
        #[cfg(feature = "amqp")]
        let serv_comm = {
            RwLock::new(ServerCommunication::new(
                &state,
                &config.amqp.url,
                &config.name,
                &config.amqp.exchange,
                &config.amqp.queue,
            ).await)
        };
        let now = Local::now();
        let conns_count = Arc::new(AtomicUsize::new(0));
        let connections_per_ip = Arc::new(RwLock::new(HashMap::new()));
        let state = MainState {
            config: config.clone(),
            user_config_idxs,
            oper_config_idxs,
            state,
            #[cfg(any(feature = "sqlite", feature = "mysql"))]
            databases,
            #[cfg(feature = "amqp")]
            serv_comm,
            conns_count,
            connections_per_ip,
            created: now.to_rfc2822(),
            created_time: now,
            #[cfg(any(feature = "sqlite", feature = "mysql"))]
            command_counts: [
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0)
            ],
            #[cfg(not(any(feature = "sqlite", feature = "mysql")))]
            command_counts: [
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),  
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0),
                AtomicU64::new(0), AtomicU64::new(0)
            ],
        };
        Ok(state)
    }

    fn count_command(&self, cmd: &Command) {
        self.command_counts[cmd.index()].fetch_add(1, Ordering::SeqCst);
    }

    // Verificar si un usuario es IRCop
    #[cfg(any(feature = "sqlite", feature = "mysql"))]
    async fn is_ircop(&self, nick: &str) -> bool {
        let state = self.state.read().await;
        if let Some(user) = state.users.get(nick) {
            user.modes.is_local_oper()
        } else {
            false
        }
    }

    // try to register connection state - print error if too many connections.
    pub(crate) async fn register_conn_state(
        &self,
        ip_addr: IpAddr,
        stream: DualTcpStream
    ) -> Option<ConnState> {
        // Check per-IP connection limit
        if let Some(max_per_ip) = self.config.max_connections_per_ip {
            let mut ip_conns = self.connections_per_ip.write().await;
            let current_per_ip = ip_conns.get(&ip_addr).copied().unwrap_or(0);
            if current_per_ip >= max_per_ip {
                error!("Too many connections from IP {} (max per IP: {})", ip_addr, max_per_ip);
                return None;
            }
            *ip_conns.entry(ip_addr).or_insert(0) += 1;
            drop(ip_conns);
        }
        
        // Check global connection limit
        if let Some(max_conns) = self.config.max_connections {
            let current = self.conns_count.load(Ordering::SeqCst);
            if current >= max_conns {
                error!("Too many total connections (max: {})", max_conns);
                // Decrement per-IP counter if we can't accept the connection
                if self.config.max_connections_per_ip.is_some() {
                    let mut ip_conns = self.connections_per_ip.write().await;
                    if let Some(count) = ip_conns.get_mut(&ip_addr) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            ip_conns.remove(&ip_addr);
                        }
                    }
                }
                return None;
            } else {
                let current = self.conns_count.fetch_add(1, Ordering::SeqCst);
                info!("Nueva conexión desde {} (total: {})", ip_addr, current + 1);
            }
        }
        
        // Create connection state
        Some(ConnState::new(ip_addr, stream, self.conns_count.clone(), self.connections_per_ip.clone()))
    }

    pub(crate) async fn remove_user(&self, conn_state: &ConnState) {
        if let Some(ref nick) = conn_state.user_state.nick {
            let mut state = self.state.write().await;
            state.remove_user(nick);
        }
        
        // Decrement per-IP connection counter
        if self.config.max_connections_per_ip.is_some() {
            let mut ip_conns = self.connections_per_ip.write().await;
            if let Some(count) = ip_conns.get_mut(&conn_state.user_state.ip_addr) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    ip_conns.remove(&conn_state.user_state.ip_addr);
                }
            }
        }
    }

    pub(crate) async fn process(&self, conn_state: &mut ConnState) -> Result<(), String> {
        // use conversion error to string to avoid problems with thread safety
        let res = self
            .process_internal(conn_state)
            .await
            .map_err(|e| e.to_string());
        conn_state.stream.flush().await.map_err(|e| e.to_string())?;
        res
    }

    pub(crate) async fn get_quit_receiver(&self) -> Fuse<oneshot::Receiver<String>> {
        let mut state = self.state.write().await;
        let (sender, receiver) = oneshot::channel();
        state.quit_sender = Some(sender);
        receiver.fuse()
    }

    async fn process_internal(&self, conn_state: &mut ConnState) -> Result<(), Box<dyn StdError + Send + Sync>> {
        tokio::select! {
            Some(msg) = conn_state.receiver.recv() => {
                conn_state.stream.feed(msg).await?;
                Ok(())
            },
            Some(_) = conn_state.ping_receiver.recv() => {
                self.feed_msg(&mut conn_state.stream, "PING :LALAL").await?;
                conn_state.run_pong_timeout(&self.config);
                Ok(())
            },
            Some(_) = conn_state.timeout_receiver.recv() => {
                info!("Pong timeout for {}", conn_state.user_state.source);
                conn_state.user_state.quit_reason = "Pong timeout".to_string();
                self.feed_msg(&mut conn_state.stream,
                            "ERROR :Pong timeout, connection will be closed.").await?;
                conn_state.quit.store(1, Ordering::SeqCst);
                Ok(())
            },
            Ok((killer, comment)) = &mut conn_state.quit_receiver => {
                let msg = format!("User killed by {}: {}", killer, comment);
                conn_state.user_state.quit_reason = msg.to_string();
                self.feed_msg(&mut conn_state.stream, msg).await?;
                conn_state.quit.store(1, Ordering::SeqCst);
                Ok(())
            },
            Ok(hostname_opt) = &mut conn_state.dns_lookup_receiver => {
                #[cfg(feature = "dns_lookup")]
                if let Some(hostname) = hostname_opt {
                    conn_state.user_state.set_hostname(hostname);
                    if let Some(nick) = &conn_state.user_state.nick {
                        let mut state = self.state.write().await;
                        if let Some(user) = state.users.get_mut(nick) {
                            user.update_hostname(&conn_state.user_state, &self.config.cloack);
                        }
                    }
                }
                #[cfg(not(feature = "dns_lookup"))]
                info!("Unexpected dns lookup: {:?}", hostname_opt);
                Ok(())
            },
            msg_str_res = conn_state.stream.next() => {
                let msg = match msg_str_res {
                    Some(Ok(ref msg_str)) => {
                        // try parse message from this line.
                        match Message::from_shared_str(msg_str) {
                            Ok(msg) => msg,
                            Err(e) => {
                                match e {
                                    MessageError::Empty => {
                                        return Ok(())   // ignore empties
                                    }
                                    MessageError::WrongSource => {
                                        self.feed_msg(&mut conn_state.stream,
                                            "ERROR :Wrong source").await?;
                                    }
                                    MessageError::NoCommand => {
                                        self.feed_msg(&mut conn_state.stream,
                                            "ERROR :No command supplied").await?;
                                    }
                                }
                                return Err(Box::new(e));
                            }
                        }
                    }
                    Some(Err(LinesCodecError::MaxLineLengthExceeded)) => {
                        let client = conn_state.user_state.client_name();
                        self.feed_msg(&mut conn_state.stream,
                                    ErrInputTooLong417{ client }).await?;
                        return Ok(())
                    },
                    Some(Err(e)) => {
                        // Manejar específicamente el caso de bytes restantes
                        if e.to_string().contains("bytes remaining on stream") {
                            info!("Bytes restantes en el stream para {}: {}", 
                                conn_state.user_state.source, e);
                            // Intentar procesar los bytes restantes
                            if let Ok(()) = conn_state.stream.flush().await {
                                return Ok(());
                            }
                            return Err(Box::new(e));
                        }
                        return Err(Box::new(e));
                    },
                    None => {
                        conn_state.user_state.quit_reason = "Unexpected eof".to_string();
                        conn_state.quit.store(1, Ordering::SeqCst);
                        return Err(Box::new(
                            io::Error::new(io::ErrorKind::UnexpectedEof, "unexpected eof")))
                    }
                };

                let cmd = match Command::from_message(&msg) {
                    Ok(cmd) => {
                        debug!("Comando recibido: {:?}", cmd);
                        cmd
                    },
                    // handle errors while parsing command.
                    Err(e) => {
                        use crate::CommandError::*;
                        let client = conn_state.user_state.client_name();
                        match e {
                            UnknownCommand(ref cmd_name) => {
                                self.feed_msg(&mut conn_state.stream,
                                        ErrUnknownCommand421{ client,
                                        command: cmd_name }).await?;
                            }
                            UnknownSubcommand(_, _)|ParameterDoesntMatch(_, _)|
                                    WrongParameter(_, _) => {
                                self.feed_msg(&mut conn_state.stream,
                                        format!("ERROR :{}", e)).await?;
                            }
                            NeedMoreParams(command) => {
                                self.feed_msg(&mut conn_state.stream,
                                        ErrNeedMoreParams461{ client,
                                        command: command.name }).await?;
                            }
                            UnknownMode(_, modechar, ref channel) => {
                                self.feed_msg(&mut conn_state.stream,
                                        ErrUnknownMode472{ client,
                                        modechar, channel }).await?;
                            }
                            UnknownUModeFlag(_) => {
                                self.feed_msg(&mut conn_state.stream,
                                        ErrUmodeUnknownFlag501{ client })
                                        .await?;
                            }
                            InvalidModeParam{ ref target, modechar, ref param,
                                    ref description } =>
                                self.feed_msg(&mut conn_state.stream,
                                        ErrInvalidModeParam696{ client,
                                        target, modechar, param, description }).await?,
                        }
                        return Ok(()); // Retornar Ok para continuar con el siguiente mensaje
                    }
                };

                self.count_command(&cmd);

                use crate::Command::*;
                // if user not authenticated
                match cmd {
                    CAP{ .. } | AUTHENTICATE{ .. } | PASS{ .. } | NICK{ .. } |
                            USER{ .. } | QUIT{ } | SETNAME{ .. } => {},
                    _ => {
                        // expect CAP, AUTHENTICATE, PASS, NICK, USER, QUIT -
                        // other commands need authenication.
                        if !conn_state.user_state.authenticated {
                            self.feed_msg(&mut conn_state.stream, ErrNotRegistered451{
                                    client: conn_state.user_state.client_name() }).await?;
                            return Ok(())
                        }
                    }
                }

                match cmd {
                    CAP{ subcommand, caps, version } =>
                        self.process_cap(conn_state, subcommand, caps, version).await,
                    AUTHENTICATE{ data } =>
                        self.process_authenticate(conn_state, data).await,
                    PASS{ password } =>
                        self.process_pass(conn_state, password).await,
                    NICK{ nickname } =>
                        self.process_nick(conn_state, nickname, &msg).await,
                    USER{ username, hostname, servername, realname } =>
                        self.process_user(conn_state, username, hostname,
                                servername, realname).await,
                    PING{ token } => self.process_ping(conn_state, token).await,
                    PONG{ token } => self.process_pong(conn_state, token).await,
                    OPER{ name, password } =>
                        self.process_oper(conn_state, name, password).await,
                    QUIT{ } => self.process_quit(conn_state).await,
                    JOIN{ channels, keys, account } =>
                        self.process_join(conn_state, channels, keys, account).await,
                    PART{ channels, reason } =>
                        self.process_part(conn_state, channels, reason).await,
                    TOPIC{ channel, topic } =>
                        self.process_topic(conn_state, channel, topic, &msg).await,
                    NAMES{ channels } =>
                        self.process_names(conn_state, channels).await,
                    LIST{ channels, server } =>
                        self.process_list(conn_state, channels, server).await,
                    INVITE{ nickname, channel } =>
                        self.process_invite(conn_state, nickname, channel, &msg).await,
                    KICK{ channel, users, comment } =>
                        self.process_kick(conn_state, channel, users, comment).await,
                    MOTD{ target } =>
                        self.process_motd(conn_state, target).await,
                    VERSION{ target } =>
                        self.process_version(conn_state, target).await,
                    ADMIN{ target } =>
                        self.process_admin(conn_state, target).await,
                    LUSERS{ } => self.process_lusers(conn_state).await,
                    TIME{ server } =>
                        self.process_time(conn_state, server).await,
                    STATS{ query, server } =>
                        self.process_stats(conn_state, query, server).await,
                    LINKS{ remote_server, server_mask } =>
                        self.process_links(conn_state, remote_server, server_mask).await,
                    HELP{ subject } =>
                        self.process_help(conn_state, subject).await,
                    INFO{ } => self.process_info(conn_state).await,
                    MODE{ target, modes } =>
                        self.process_mode(conn_state, target, modes).await,
                    PRIVMSG{ targets, text } =>
                        self.process_privmsg(conn_state, targets, text).await,
                    NOTICE{ targets, text } =>
                        self.process_notice(conn_state, targets, text).await,
                    WHO{ mask } => self.process_who(conn_state, mask).await,
                    WHOIS{ target, nickmasks } =>
                        self.process_whois(conn_state, target, nickmasks).await,
                    WHOWAS{ nickname, count, server } =>
                        self.process_whowas(conn_state, nickname, count, server).await,
                    KILL{ nickname, comment } =>
                        self.process_kill(conn_state, nickname, comment).await,
                    REHASH{ } => self.process_rehash(conn_state).await,
                    RESTART{ } => self.process_restart(conn_state).await,
                    AWAY{ text } =>
                        self.process_away(conn_state, text).await,
                    USERHOST{ nicknames } =>
                        self.process_userhost(conn_state, nicknames).await,
                    WALLOPS{ .. } =>
                        self.process_wallops(conn_state, &msg).await,
                    ISON{ nicknames } =>
                        self.process_ison(conn_state, nicknames).await,
                    DIE{ message } =>
                        self.process_die(conn_state, message).await,
                    SERVERS{ target } => 
                        self.process_servers(conn_state, target.as_deref()).await,
                    #[cfg(any(feature = "sqlite", feature = "mysql"))]
                    NICKSERV{ subcommand, params } =>
                        self.process_nickserv(conn_state, subcommand, params).await,
                    #[cfg(any(feature = "sqlite", feature = "mysql"))]
                    NS{ subcommand, params } =>
                        self.process_nickserv(conn_state, subcommand, params).await,
                    #[cfg(any(feature = "sqlite", feature = "mysql"))]
                    CHANSERV{ subcommand, params } =>
                        self.process_chanserv(conn_state, subcommand, params).await,
                    #[cfg(any(feature = "sqlite", feature = "mysql"))]
                    CS{ subcommand, params } =>
                        self.process_chanserv(conn_state, subcommand, params).await,
                    SETNAME { realname } =>
                        self.process_setname(conn_state, realname).await,
                    MONITOR { subcommand, targets } =>
                        self.process_monitor(conn_state, subcommand, targets).await,
                }
            },
        }
    }

    // helper to feed messages
    async fn feed_msg<T: fmt::Display>(
        &self,
        stream: &mut BufferedLineStream,
        t: T,
    ) -> Result<(), LinesCodecError> {
        let message = format!(":{} {}", self.config.name, t);
        stream.feed(message).await
    }

    // helper to feed messages
    async fn feed_msg_source<T: fmt::Display>(
        &self,
        stream: &mut BufferedLineStream,
        source: &str,
        t: T,
    ) -> Result<(), LinesCodecError> {
        stream.feed(format!(":{} {}", source, t)).await
    }

    async fn process_servers<'a>(
        &self,
        conn_state: &mut ConnState,
        _target: Option<&'a str>,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        // Verificar si el usuario tiene privilegios de operador
        let state = self.state.read().await;
        let user = state.users.get(conn_state.user_state.nick.as_ref().unwrap()).unwrap();
        if !user.modes.is_local_oper() {
            self.feed_msg(&mut conn_state.stream, 
                ErrNoPrivileges481{ client: conn_state.user_state.client_name() }).await?;
            return Ok(());
        }

        // Enviar el final de la lista
        self.feed_msg(&mut conn_state.stream, 
            format!(":{} 365 {} :End of /SERVERS list", 
                self.config.name,
                conn_state.user_state.nick.as_ref().unwrap())).await?;

        Ok(())
    }
}

// main process to handle commands from client.
async fn user_state_process(main_state: Arc<MainState>, stream: DualTcpStream, addr: SocketAddr) {
    if let Some(mut conn_state) = main_state.register_conn_state(addr.ip(), stream).await {
        #[cfg(feature = "dns_lookup")]
        if main_state.config.dns_lookup {
            let _ = main_state.feed_msg(
                &mut conn_state.stream,
                "NOTICE IP_LOOKUP :*** Looking up hostname.",
            )
            .await;
            conn_state.run_dns_lookup();
        }
        #[cfg(not(feature = "dns_lookup"))]
        if main_state.config.dns_lookup {
            error!("DNS lookup is not enabled!");
        }

        while !conn_state.is_quit() {
            match main_state.process(&mut conn_state).await {
                Ok(_) => {
                    // Solo actualizamos los modos después de que el usuario esté autenticado
                    if conn_state.user_state.authenticated {
                        if let Some(nick) = &conn_state.user_state.nick {
                            let mut state = main_state.state.write().await;
                            if let Some(user) = state.users.get_mut(nick) {
                                if conn_state.is_secure() {
                                    user.modes.secure = true;
                                }
                                if conn_state.is_websocket() {
                                    user.modes.websocket = true;
                                }
                                user.source = format!("{}!{}@{}",
                                    nick, user.name, user.cloack.clone());
                                conn_state.user_state.source = user.source.clone();
                            }
                        }
                    }
                    continue;
                },
                Err(e) => {
                    if e.to_string().contains("unexpected eof") {
                        info!("Conexión cerrada por el cliente: {}", conn_state.user_state.source);
                        conn_state.user_state.quit_reason = "Unexpected eof".to_string();
                    } else {
                        error!("Error para {}: {}", conn_state.user_state.source, e);
                    }
                    break;
                }
            }
        }

        // Obtener el nick del usuario que se va
        if let Some(nick) = &conn_state.user_state.nick {
            // Primero obtenemos una copia de los canales del usuario
            let user_channels = {
                let state = main_state.state.read().await;
                if let Some(user) = state.users.get(nick) {
                    user.channels.clone()
                } else {
                    HashSet::new()
                }
            };

            // Notificar a todos los usuarios en los canales compartidos
            for channel in &user_channels {
                let channel_users = {
                    let state = main_state.state.read().await;
                    if let Some(chanobj) = state.channels.get(&channel.to_string()) {
                        chanobj.users.keys().cloned().collect::<Vec<_>>()
                    } else {
                        continue;
                    }
                };

                for nickname in channel_users {
                    if nickname != nick.as_str() {
                        let state = main_state.state.read().await;
                        if let Some(user) = state.users.get(&nickname) {
                            let _ = user.send_msg_display(
                                &conn_state.user_state.source,
                                format!("QUIT :{}", conn_state.user_state.quit_reason),
                            );
                        }
                    }
                }
            }
        }
        // Asegurarnos de limpiar correctamente el usuario del estado global
        info!(
            "User {} gone from server",
            conn_state.user_state.source,
        );
        main_state.remove_user(&conn_state).await;

        // IMPORTANTE: Decrementar el contador de conexiones activas
        main_state.conns_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        
        // Decrement per-IP connection counter
        if main_state.config.max_connections_per_ip.is_some() {
            let mut ip_conns = main_state.connections_per_ip.write().await;
            if let Some(count) = ip_conns.get_mut(&conn_state.user_state.ip_addr) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    ip_conns.remove(&conn_state.user_state.ip_addr);
                }
            }
        }
    }
}

#[cfg(feature = "tls")]
async fn user_state_process_tls_prepare(
    stream: TcpStream,
    acceptor: Arc<SslAcceptor>,
) -> Result<SslStream<TcpStream>, String> {
    let ssl = Ssl::new(acceptor.context()).map_err(|e| e.to_string())?;
    let mut tls_stream = SslStream::new(ssl, stream).map_err(|e| e.to_string())?;
    use std::pin::Pin;
    
    Pin::new(&mut tls_stream)
        .accept()
        .await
        .map_err(|e| e.to_string())?;
    Ok(tls_stream)
}

#[cfg(feature = "tls")]
async fn user_state_process_tls(
    main_state: Arc<MainState>,
    stream: TcpStream,
    acceptor: Arc<SslAcceptor>,
    addr: SocketAddr,
) {
    match user_state_process_tls_prepare(stream, acceptor).await {
        Ok(stream) => {
            user_state_process(main_state, DualTcpStream::SecureStream(stream), addr).await;
        }
        Err(e) => {
            error!("Failed to prepare TLS connection: {}", e);
        }
    }
}

pub(crate) fn initialize_logging(config: &MainConfig) {
    use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};
    let s = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(config.log_level.into()))
        .with_span_events(FmtSpan::FULL)
        .with_file(true)
        .with_line_number(true)
        .with_thread_ids(true)
        // disable ansi color for files
        .with_ansi(config.log_file.is_none());
    if let Some(ref log_file) = config.log_file {
        if let Ok(f) = File::create(log_file) {
            s.with_writer(f).init();
        } else {
            error!("No log file {}", log_file);
            s.init()
        }
    } else {
        s.init();
    }
}

#[cfg(feature = "dns_lookup")]
lazy_static! {
    static ref DNS_RESOLVER: std::sync::RwLock<Option<Arc::<TokioAsyncResolver>>> =
        std::sync::RwLock::new(None);
}

#[cfg(feature = "dns_lookup")]
fn initialize_dns_resolver() {
    let mut r = DNS_RESOLVER.write().unwrap();
    if r.is_none() {
        *r = Some(Arc::new(
            {
                // for windows or linux
                #[cfg(any(unix, windows))]
                {
                    // use the system resolver configuration
                    TokioAsyncResolver::from_system_conf(TokioHandle)
                }

                // for other
                #[cfg(not(any(unix, windows)))]
                {
                    // Directly reference the config types
                    use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};

                    // Get a new resolver with the google nameservers as
                    // the upstream recursive resolvers
                    TokioAsyncResolver::tokio(ResolverConfig::google(), ResolverOpts::default())
                }
            }
            .expect("failed to create resolver"),
        ));
    }
}

#[cfg(feature = "dns_lookup")]
pub(self) fn dns_lookup(sender: oneshot::Sender<Option<String>>, ip: IpAddr) {
    let r = DNS_RESOLVER.read().unwrap();
    let resolver = (*r).clone().unwrap();
    tokio::spawn(dns_lookup_process(resolver, sender, ip));
}

#[cfg(feature = "dns_lookup")]
async fn dns_lookup_process(
    resolver: Arc<TokioAsyncResolver>,
    sender: oneshot::Sender<Option<String>>,
    ip: IpAddr,
) {
    let r = match resolver.reverse_lookup(ip).await {
        Ok(lookup) => {
            if let Some(x) = lookup.iter().next() {
                let namex = x.to_string();
                let name = if namex.as_bytes()[namex.len() - 1] == b'.' {
                    namex[..namex.len() - 1].to_string()
                } else {
                    namex
                };
                sender.send(Some(name))
            } else {
                sender.send(None)
            }
        }
        Err(_) => sender.send(None),
    };
    if r.is_err() {
        error!("Error while sending dns lookup");
    }
}

async fn handle_websocket_connection(
    stream: TcpStream,
    _addr: SocketAddr,
    _tls_config: Option<TLSConfig>,
) -> Result<DualTcpStream, Box<dyn Error + Send + Sync>> {
    #[cfg(feature = "tls")]
    {
        if let Some(tlsconfig) = _tls_config {
            let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
            acceptor.set_private_key_file(&tlsconfig.cert_key_file, SslFiletype::PEM)?;
            acceptor.set_certificate_chain_file(&tlsconfig.cert_file)?;
            let acceptor = Arc::new(acceptor.build());
            
            let ssl = Ssl::new(acceptor.context()).map_err(|e| e.to_string())?;
            let mut tls_stream = SslStream::new(ssl, stream).map_err(|e| e.to_string())?;
            use std::pin::Pin;
            Pin::new(&mut tls_stream).accept().await.map_err(|e| e.to_string())?;
            
            // Configurar el handshake con los protocolos soportados
            return match timeout(Duration::from_secs(10), tokio_tungstenite::accept_async(tls_stream)).await {
                Ok(wss_stream) => Ok(DualTcpStream::SecureWebSocketStream(wss_stream?)),
                Err(e) => {
                    error!("Error en handshake SecureWebSocket: {:?}", e);
                    Err(e.into())
                }
            };
        }
    }
    // Configurar el handshake con los protocolos soportados
    match timeout(Duration::from_secs(10), tokio_tungstenite::accept_async(stream)).await {
        Ok(ws_stream) => Ok(DualTcpStream::WebSocketStream(ws_stream?)),
        Err(e) => {
            error!("Error en handshake WebSocket: {:?}", e);
            Err(e.into())
        }
    }
}

// main routine to run server
pub(crate) async fn run_server(
    config: MainConfig,
) -> Result<(Arc<MainState>, Vec<JoinHandle<()>>), Box<dyn Error>> {
    #[cfg(feature = "dns_lookup")]
    if config.dns_lookup {
        initialize_dns_resolver();
    }

    let main_state = Arc::new(MainState::new_from_config(config.clone()).await?);
    let main_state_to_return = main_state.clone();

    let mut handles = Vec::new();
    for listener_config in config.listeners {
        let main_state = main_state.clone();
        let listener = TcpListener::bind((listener_config.listen, listener_config.port)).await?;
        let handle = if listener_config.tls.is_some() && !listener_config.websocket {
            #[cfg(feature = "tls")]
            {
                let cloned_tls = listener_config.tls.clone();
                let tlsconfig = cloned_tls.unwrap();
                let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls())?;
                acceptor.set_private_key_file(tlsconfig.cert_key_file, SslFiletype::PEM)?;
                acceptor.set_certificate_chain_file(tlsconfig.cert_file)?;
                let acceptor = Arc::new(acceptor.build());

                tokio::spawn(async move {
                    let mut quit_receiver = main_state.get_quit_receiver().await;
                    info!("Listen TLS {} on port: {}", listener_config.listen, listener_config.port);
                    loop {
                        tokio::select! {
                            res = listener.accept() => {
                                match res {
                                    Ok((stream, addr)) => {
                                        tokio::spawn(user_state_process_tls(main_state.clone(),
                                                stream, acceptor.clone(), addr));
                                    }
                                    Err(e) => { error!("Accept connection error: {}", e); }
                                };
                            }
                            Ok(msg) = &mut quit_receiver => {
                                info!("Server quit: {}", msg);
                                std::process::exit(0);
                            }
                        };
                    }
                })
            }

            #[cfg(not(any(feature = "tls")))]
            tokio::spawn(async move { error!("Unsupported TLS") })
        } else if listener_config.websocket {
            let tls_config = listener_config.tls.clone();
            tokio::spawn(async move {
                let mut quit_receiver = main_state.get_quit_receiver().await;
                info!("Listen Websocket {} on port: {}", listener_config.listen, listener_config.port);
                loop {
                    tokio::select! {
                        res = listener.accept() => {
                            match res {
                                Ok((stream, addr)) => {
                                    match handle_websocket_connection(stream, addr, tls_config.clone()).await {
                                        Ok(ws_stream) => {
                                            tokio::spawn(user_state_process(main_state.clone(),
                                                    ws_stream, addr));
                                        }
                                        Err(e) => error!("Error en handshake de WebSocket: {}", e),
                                    }
                                }
                                Err(e) => { error!("Error al aceptar conexión: {}", e); }
                            };
                        }
                        Ok(msg) = &mut quit_receiver => {
                            info!("Server quit: {}", msg);
                            std::process::exit(0);
                        }
                    };
                }
            })
        } else {
            tokio::spawn(async move {
                let mut quit_receiver = main_state.get_quit_receiver().await;
                info!("Listen Plain {} on port: {}", listener_config.listen, listener_config.port);
                loop {
                    tokio::select! {
                        res = listener.accept() => {
                            match res {
                                Ok((stream, addr)) => {
                                    tokio::spawn(user_state_process(main_state.clone(),
                                            DualTcpStream::PlainStream(stream), addr));
                                }
                                Err(e) => { error!("Accept connection error: {}", e); }
                            };
                        }
                        Ok(msg) = &mut quit_receiver => {
                            info!("Server quit: {}", msg);
                            std::process::exit(0);
                        }
                    };
                }
            })
        };
        handles.push(handle);
    }

    #[cfg(feature = "amqp")]
    let _ = main_state.serv_comm.write().await.connect().await;
    #[cfg(feature = "amqp")]
    let _ = main_state.serv_comm.write().await.start_consuming().await;

    Ok((main_state_to_return, handles))
}

#[cfg(test)]
mod test {
    use super::*;
    pub(crate) use futures::SinkExt;
    pub(crate) use std::collections::HashSet;
    pub(crate) use std::iter::FromIterator;
    pub(crate) use std::time::Duration;
    use tokio::net::TcpStream;
    pub(crate) use tokio::time;

    use std::sync::atomic::AtomicU16;

    static PORT_COUNTER: AtomicU16 = AtomicU16::new(7888);
    //use std::sync::Once;
    //static LOGGING_START: Once = Once::new();

    pub(crate) async fn run_test_server(
        config: MainConfig,
    ) -> (Arc<MainState>, JoinHandle<()>, u16) {
        let mut config = config;
        let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
        config.listeners.iter_mut().for_each(|l| l.port = port);
        let (main_state, handle) = run_server(config).await.unwrap();
        (main_state, handle, port)
    }

    pub(crate) async fn quit_test_server(main_state: Arc<MainState>, handle: JoinHandle<()>) {
        main_state
            .state
            .write()
            .await
            .quit_sender
            .take()
            .unwrap()
            .send("Test".to_string())
            .unwrap();
        handle.await.unwrap();
    }

    pub(crate) async fn connect_to_test(port: u16) -> Framed<TcpStream, IRCLinesCodec> {
        let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        Framed::new(stream, IRCLinesCodec::new_with_max_length(2000))
    }

    pub(crate) async fn login_to_test<'a>(
        port: u16,
        nick: &'a str,
        name: &'a str,
        realname: &'a str,
    ) -> Framed<TcpStream, IRCLinesCodec> {
        let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut line_stream = Framed::new(stream, IRCLinesCodec::new_with_max_length(2000));
        line_stream.send(format!("NICK {}", nick)).await.unwrap();
        line_stream
            .send(format!("USER {} 8 * :{}", name, realname))
            .await
            .unwrap();
        line_stream
    }

    pub(crate) async fn login_to_test_and_skip<'a>(
        port: u16,
        nick: &'a str,
        name: &'a str,
        realname: &'a str,
    ) -> Framed<TcpStream, IRCLinesCodec> {
        let mut line_stream = login_to_test(port, nick, name, realname).await;
        for _ in 0..18 {
            line_stream.next().await.unwrap().unwrap();
        }
        line_stream
    }

    #[cfg(feature = "tls")]
    fn get_cert_file_path() -> String {
        let mut path = PathBuf::new();
        path.push(env!("CARGO_MANIFEST_DIR"));
        path.push("test_data");
        path.push("cert.crt");
        path.to_string_lossy().to_string()
    }

    #[cfg(feature = "tls")]
    fn get_cert_key_file_path() -> String {
        let mut path = PathBuf::new();
        path.push(env!("CARGO_MANIFEST_DIR"));
        path.push("test_data");
        path.push("cert_key.crt");
        path.to_string_lossy().to_string()
    }

    #[cfg(feature = "tls")]
    pub(crate) async fn run_test_tls_server(
        config: MainConfig,
    ) -> (Arc<MainState>, JoinHandle<()>, u16) {
        //LOGGING_START.call_once(|| {
        //    initialize_logging(&MainConfig::default());
        //});
        let mut config = config;
        config.tls = Some(TLSConfig {
            cert_file: get_cert_file_path(),
            cert_key_file: get_cert_key_file_path(),
        });
        config.port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let port = config.port;
        let (main_state, handle) = run_server(config).await.unwrap();
        (main_state, handle, port)
    }

    #[cfg(feature = "tls")]
    pub(crate) async fn connect_to_test_tls(
        port: u16,
    ) -> Framed<SslStream<TcpStream>, IRCLinesCodec> {
        let mut connector = SslAcceptor::builder(SslMethod::tls()).unwrap();
        connector.set_ca_file(get_cert_file_path()).unwrap();

        let ssl = connector
            .build()
            .configure()
            .unwrap()
            .into_ssl("localhost")
            .unwrap();

        let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut tls_stream = SslStream::new(ssl, stream).unwrap();
        use std::pin::Pin;
        Pin::new(&mut tls_stream).connect().await.unwrap();
        Framed::new(tls_stream, IRCLinesCodec::new_with_max_length(2000))
    }

    #[cfg(feature = "tls")]
    pub(crate) async fn login_to_test_tls<'a>(
        port: u16,
        nick: &'a str,
        name: &'a str,
        realname: &'a str,
    ) -> Framed<SslStream<TcpStream>, IRCLinesCodec> {
        let mut line_stream = connect_to_test_tls(port).await;
        line_stream.send(format!("NICK {}", nick)).await.unwrap();
        line_stream
            .send(format!("USER {} 8 * :{}", name, realname))
            .await
            .unwrap();
        line_stream
    }

    #[cfg(feature = "tls")]
    pub(crate) async fn login_to_test_tls_and_skip<'a>(
        port: u16,
        nick: &'a str,
        name: &'a str,
        realname: &'a str,
    ) -> Framed<SslStream<TcpStream>, IRCLinesCodec> {
        let mut line_stream = login_to_test_tls(port, nick, name, realname).await;
        for _ in 0..18 {
            line_stream.next().await.unwrap().unwrap();
        }
        line_stream
    }

    #[cfg(feature = "tls")]
    #[tokio::test]
    async fn test_server_tls_first() {
        let (main_state, handle, port) = run_test_tls_server(MainConfig::default()).await;
        {
            let mut line_stream = login_to_test_tls(port, "mati", "mat", "MatiSzpaki").await;
            assert_eq!(
                ":irc.irc 001 mati :Welcome to the IRCnetwork \
                    Network, mati!~mat@127.0.0.1"
                    .to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                concat!(
                    ":irc.irc 002 mati :Your host is irc.irc, running \
                    version ",
                    env!("CARGO_PKG_NAME"),
                    "-",
                    env!("CARGO_PKG_VERSION")
                )
                .to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                format!(
                    ":irc.irc 003 mati :This server was created {}",
                    main_state.created
                ),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                concat!(
                    ":irc.irc 004 mati irc.irc ",
                    env!("CARGO_PKG_NAME"),
                    "-",
                    env!("CARGO_PKG_VERSION"),
                    " Oiorw IabehiklmnopqstvB"
                ),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 005 mati AWAYLEN=1000 CASEMAPPING=ascii \
                    CHANMODES=IabehiklmnopqstvB CHANNELLEN=1000 CHANTYPES=&# EXCEPTS=e FNC \
                    HOSTLEN=1000 INVEX=I KEYLEN=1000 :are supported by this server"
                    .to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 005 mati KICKLEN=1000 LINELEN=2000 MAXLIST=beI:1000 \
                    MAXNICKLEN=200 MAXPARA=500 MAXTARGETS=500 MODES=500 NETWORK=IRCnetwork \
                    NICKLEN=200 PREFIX=(qaohv)~&@%+ :are supported by this server"
                    .to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 005 mati SAFELIST STATUSMSG=~&@%+ TOPICLEN=1000 USERLEN=200 \
                    USERMODES=OiorwWz :are supported by this server"
                    .to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 251 mati :There are 1 users and 0 invisible \
                    on 1 servers"
                    .to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 252 mati 0 :operator(s) online".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 253 mati 0 :unknown connection(s)".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 254 mati 0 :channels formed".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 255 mati :I have 1 clients and 1 servers".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 265 mati 1 1 :Current local users 1, max 1".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 266 mati 1 1 :Current global users 1, max 1".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 375 mati :- irc.irc Message of the day - ".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 372 mati :Hello, world!".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 376 mati :End of /MOTD command.".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            assert_eq!(
                ":irc.irc 221 mati +".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
        }

        quit_test_server(main_state, handle).await;
    }

    #[cfg(feature = "tls")]
    #[tokio::test]
    async fn test_server_timeouts() {
        let (main_state, handle, port) = run_test_server(MainConfig::default()).await;

        {
            let mut line_stream = login_to_test_and_skip(port, "mati", "mat", "MatiSzpaki").await;

            line_stream.send("PING :bumbum".to_string()).await.unwrap();
            assert_eq!(
                ":irc.irc PONG irc.irc :bumbum".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            time::pause();
            time::advance(Duration::from_millis(119900)).await;
            time::resume();
            assert_eq!(
                ":irc.irc PING :LALAL".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            line_stream.send("PONG :LALAL".to_string()).await.unwrap();

            // test timeout
            time::pause();
            time::advance(Duration::from_millis(119900)).await;
            time::resume();
            assert_eq!(
                ":irc.irc PING :LALAL".to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
            time::pause();
            time::advance(Duration::from_millis(19900)).await;
            time::resume();
            assert_eq!(
                ":irc.irc ERROR :Pong timeout, connection will \
                be closed."
                    .to_string(),
                line_stream.next().await.unwrap().unwrap()
            );
        }

        quit_test_server(main_state, handle).await;
    }
}

mod channel_cmds;
mod conn_cmds;
mod rest_cmds;
mod srv_query_cmds;

#[cfg(any(feature = "sqlite", feature = "mysql"))]
pub mod nickserv;
#[cfg(any(feature = "sqlite", feature = "mysql"))]
pub mod chanserv;

#[cfg(feature = "amqp")]
pub mod server_communication;