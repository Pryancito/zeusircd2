// config.rs - configuration
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

use serde::Deserializer;
use serde_derive::Deserialize;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::net::IpAddr;
use std::str::FromStr;
use validator::Validate;
use clap::error::ErrorKind;

use crate::utils::match_wildcard;
use crate::utils::validate_channel;
use crate::utils::validate_password_hash;
use crate::utils::validate_username;

#[derive(clap::Parser, Clone)]
#[clap(author, version, about, long_about = None)]
pub(crate) struct Cli {
    #[clap(short, long, help = "Generate password hash")]
    pub(crate) gen_password_hash: bool,
    #[clap(short = 'P', long, help = "Password for generated password hash")]
    pub(crate) password: Option<String>,
    #[clap(short, long, help = "Configuration file path")]
    config: Option<String>,
    #[clap(short, long, help = "Listen bind address")]
    listen: Option<IpAddr>,
    #[clap(short, long, help = "Listen port")]
    port: Option<u16>,
    #[clap(short = 'n', long, help = "Server name")]
    name: Option<String>,
    #[clap(short = 'N', long, help = "Network")]
    network: Option<String>,
    #[clap(short, long, help = "DNS lookup if client connects")]
    dns_lookup: bool,
    #[clap(short = 'C', long, help = "TLS certificate file")]
    tls_cert_file: Option<String>,
    #[clap(short = 'K', long, help = "TLS certificate key file")]
    tls_cert_key_file: Option<String>,
    #[clap(short = 'L', long, help = "Log file path")]
    log_file: Option<String>,
}

#[derive(PartialEq, Eq, Deserialize, Debug, Clone)]
pub(crate) struct TLSConfig {
    pub(crate) cert_file: String,
    pub(crate) cert_key_file: String,
}

#[derive(PartialEq, Eq, Deserialize, Debug, Validate, Clone)]
pub(crate) struct OperatorConfig {
    #[validate(custom(function = "validate_username"))]
    pub(crate) name: String,
    #[validate(custom(function = "validate_password_hash"))]
    pub(crate) password: String,
    pub(crate) mask: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, Deserialize, Debug, Default, Validate)]
pub(crate) struct UserModes {
    pub(crate) invisible: bool,
    pub(crate) oper: bool,
    pub(crate) local_oper: bool,
    pub(crate) registered: bool,
    pub(crate) wallops: bool,
    pub(crate) websocket: bool,
    pub(crate) secure: bool,
    pub(crate) cloacked: bool,
}

#[derive(Clone, PartialEq, Eq, Deserialize, Debug, Validate)]
pub(crate) struct DB {
    pub database: String, // "sqlite" or "mysql"
    pub url: String,
}

impl fmt::Display for UserModes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = '+'.to_string();
        if self.invisible {
            s.push('i');
        }
        if self.oper {
            s.push('o');
        }
        if self.local_oper {
            s.push('O');
        }
        if self.registered {
            s.push('r');
        }
        if self.wallops {
            s.push('w');
        }
        if self.websocket {
            s.push('W');
        }
        if self.secure {
            s.push('z');
        }
        if self.cloacked {
            s.push('x');
        }
        f.write_str(&s)
    }
}

impl UserModes {
    pub(crate) fn is_local_oper(&self) -> bool {
        self.local_oper || self.oper
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize, Debug, Validate, Default)]
pub(crate) struct ChannelModes {
    // If channel modes we use Option to avoid unnecessary field definition if list
    // in this field should be. The administrator can omit fields for empty lists.
    pub(crate) ban: Option<HashSet<String>>,
    pub(crate) global_ban: Option<HashSet<String>>,
    pub(crate) exception: Option<HashSet<String>>,
    pub(crate) client_limit: Option<usize>,
    pub(crate) invite_exception: Option<HashSet<String>>,
    pub(crate) key: Option<String>,
    pub(crate) operators: Option<HashSet<String>>,
    pub(crate) half_operators: Option<HashSet<String>>,
    pub(crate) voices: Option<HashSet<String>>,
    pub(crate) founders: Option<HashSet<String>>,
    pub(crate) protecteds: Option<HashSet<String>>,
    pub(crate) invite_only: bool,
    pub(crate) moderated: bool,
    pub(crate) secret: bool,
    pub(crate) protected_topic: bool,
    pub(crate) no_external_messages: bool,
}

impl ChannelModes {
    // create new channel modes for new channel created by user. By default,
    // user that created channel is founder and operator in this channel.
    pub(crate) fn new_for_channel(user_nick: String) -> Self {
        ChannelModes {
            operators: Some([user_nick.clone()].into()),
            founders: Some([user_nick].into()),
            ..ChannelModes::default()
        }
    }

    pub(crate) fn banned(&self, source: &str) -> bool {
        // Check local bans
        let local_banned = self.ban
            .as_ref()
            .map_or(false, |b| b.iter().any(|b| {
                // Extraer la parte de la máscara sin el tiempo
                let mask = if let Some(idx) = b.find('|') {
                    &b[..idx]
                } else {
                    b
                };
                match_wildcard(mask, source)
            }));
        
        // Check global bans
        let global_banned = self.global_ban
            .as_ref()
            .map_or(false, |b| b.iter().any(|b| {
                // Extraer la parte de la máscara sin el tiempo
                let mask = if let Some(idx) = b.find('|') {
                    &b[..idx]
                } else {
                    b
                };
                match_wildcard(mask, source)
            }));

        // User is banned if either local or global ban matches
        (local_banned || global_banned) && 
        // But not if there's an exception
        (!self
            .exception
            .as_ref()
            .map_or(false, |e| e.iter().any(|e| match_wildcard(e, source))))
    }

    // rename user - just rename nick in lists.
    pub(crate) fn rename_user(&mut self, old_nick: &String, nick: String) {
        if let Some(ref mut operators) = self.operators {
            if operators.remove(old_nick) {
                operators.insert(nick.clone());
            }
        }
        if let Some(ref mut half_operators) = self.half_operators {
            if half_operators.remove(old_nick) {
                half_operators.insert(nick.clone());
            }
        }
        if let Some(ref mut voices) = self.voices {
            if voices.remove(old_nick) {
                voices.insert(nick.clone());
            }
        }
        if let Some(ref mut founders) = self.founders {
            if founders.remove(old_nick) {
                founders.insert(nick.clone());
            }
        }
        if let Some(ref mut protecteds) = self.protecteds {
            if protecteds.remove(old_nick) {
                protecteds.insert(nick);
            }
        }
    }
}

impl fmt::Display for ChannelModes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = '+'.to_string();
        if self.invite_only {
            s.push('i');
        }
        if self.moderated {
            s.push('m');
        }
        if self.secret {
            s.push('s');
        }
        if self.protected_topic {
            s.push('t');
        }
        if self.no_external_messages {
            s.push('n');
        }
        if self.key.is_some() {
            s.push('k');
        }
        if self.client_limit.is_some() {
            s.push('l');
        }
        if let Some(ref k) = self.key {
            s.push(' ');
            s += k;
        }
        if let Some(l) = self.client_limit {
            s.push(' ');
            s += &l.to_string();
        }
        if let Some(ref ban) = self.ban {
            ban.iter().for_each(|b| {
                s += " +b ";
                s += b;
            });
        }
        if let Some(ref gban) = self.global_ban {
            gban.iter().for_each(|b| {
                s += " +B ";
                s += b;
            });
        }
        if let Some(ref exception) = self.exception {
            exception.iter().for_each(|e| {
                s += " +e ";
                s += e;
            });
        }
        if let Some(ref invite_exception) = self.invite_exception {
            invite_exception.iter().for_each(|i| {
                s += " +I ";
                s += i;
            });
        }

        if let Some(ref founders) = self.founders {
            founders.iter().for_each(|q| {
                s += " +q ";
                s += q;
            });
        }
        if let Some(ref protecteds) = self.protecteds {
            protecteds.iter().for_each(|a| {
                s += " +a ";
                s += a;
            });
        }
        if let Some(ref operators) = self.operators {
            operators.iter().for_each(|o| {
                s += " +o ";
                s += o;
            });
        }
        if let Some(ref half_operators) = self.half_operators {
            half_operators.iter().for_each(|h| {
                s += " +h ";
                s += h;
            });
        }
        if let Some(ref voices) = self.voices {
            voices.iter().for_each(|v| {
                s += " +v ";
                s += v;
            });
        }
        f.write_str(&s)
    }
}

#[derive(Clone, PartialEq, Eq, Deserialize, Debug, Validate)]
pub(crate) struct ChannelConfig {
    #[validate(custom(function = "validate_channel"))]
    pub(crate) name: String,
    pub(crate) topic: Option<String>,
    #[validate(nested)]
    pub(crate) modes: ChannelModes,
}

#[derive(PartialEq, Eq, Deserialize, Debug, Validate, Clone)]
pub(crate) struct UserConfig {
    #[validate(custom(function = "validate_username"))]
    pub(crate) name: String,
    #[validate(custom(function = "validate_username"))]
    pub(crate) nick: String,
    #[validate(length(min = 6))]
    #[validate(custom(function = "validate_password_hash"))]
    pub(crate) password: Option<String>,
    pub(crate) mask: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Deserialize, Debug, Validate)]
pub(crate) struct ListenerConfig {
    pub(crate) listen: IpAddr,
    pub(crate) port: u16,
    pub(crate) tls: Option<TLSConfig>,
    pub(crate) websocket: bool,
}

/// Main configuration structure.
#[derive(Clone, PartialEq, Eq, Deserialize, Debug, Validate)]
pub(crate) struct MainConfig {
    #[validate(contains(pattern = "."))]
    pub(crate) name: String,
    pub(crate) admin_info: String,
    pub(crate) admin_info2: Option<String>,
    pub(crate) admin_email: Option<String>,
    pub(crate) info: String,
    pub(crate) motd: String,
    pub(crate) listeners: Vec<ListenerConfig>,
    pub(crate) network: String,
    #[validate(custom(function = "validate_password_hash"))]
    pub(crate) password: Option<String>,
    pub(crate) max_connections: Option<usize>,
    pub(crate) max_joins: Option<usize>,
    pub(crate) ping_timeout: u64,
    pub(crate) pong_timeout: u64,
    pub(crate) dns_lookup: bool,
    #[validate(nested)]
    pub(crate) default_user_modes: UserModes,
    pub(crate) log_file: Option<String>,
    #[serde(deserialize_with = "tracing_log_level_deserialize")]
    pub(crate) log_level: tracing::Level,
    pub(crate) database: Option<DB>,
    #[validate(nested)]
    pub(crate) operators: Option<Vec<OperatorConfig>>,
    #[validate(nested)]
    pub(crate) users: Option<Vec<UserConfig>>,
    #[validate(nested)]
    pub(crate) channels: Option<Vec<ChannelConfig>>,
    #[cfg(feature = "amqp")]
    pub(crate) amqp: AmqpConfig,
    pub(crate) cloack: Cloacked,
}

#[derive(PartialEq, Eq, Deserialize, Debug, Validate, Clone)]
pub struct Cloacked {
    pub key1: String,
    pub key2: String,
    pub key3: String,
    pub prefix: String,
}

#[cfg(feature = "amqp")]
#[derive(PartialEq, Eq, Deserialize, Debug, Validate, Clone)]
pub struct AmqpConfig {
    pub url: String,
    pub exchange: String,
    pub queue: String,
}

#[derive(PartialEq, Eq, Deserialize, Debug, Validate, Clone)]
pub struct ServerConfig {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub description: String,
}

#[derive(PartialEq, Eq, Deserialize, Debug, Clone)]
pub struct ServerLink {
    pub name: String,
    pub address: String,
    pub hop_count: u32,
    pub description: String,
}

struct TracingLevelVisitor;

impl<'de> serde::de::Visitor<'de> for TracingLevelVisitor {
    type Value = tracing::Level;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("TracingLevel")
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        tracing::Level::from_str(v).map_err(|e| serde::de::Error::custom(e))
    }
}

fn tracing_log_level_deserialize<'de, D: Deserializer<'de>>(
    ds: D,
) -> Result<tracing::Level, D::Error> {
    ds.deserialize_str(TracingLevelVisitor)
}

impl MainConfig {
    // create new main config from command line.
    pub(crate) fn new(cli: Cli) -> Result<MainConfig, Box<dyn Error>> {
        // get config path.
        let config_path = cli.config.as_deref().unwrap_or("simple-irc-server.toml");
        let mut config_file = File::open(config_path)?;
        let mut config_str = String::new();
        config_file.read_to_string(&mut config_str)?;
        // modify configuration by CLI options
        {
            let mut config: MainConfig = toml::from_str(&config_str)?;
            if let Some(addr) = cli.listen {
                config.listeners.iter_mut().for_each(|l| l.listen = addr);
            }
            if let Some(port) = cli.port {
                config.listeners.iter_mut().for_each(|l| l.port = port);
            }
            if let Some(name) = cli.name {
                config.name = name;
            }
            if let Some(network) = cli.network {
                config.network = network;
            }
            if let Some(log_file) = cli.log_file {
                config.log_file = Some(log_file)
            }
            config.dns_lookup = config.dns_lookup || cli.dns_lookup;

            // get indicator to check later
            let (have_cert, have_cert_key) =
                (cli.tls_cert_file.is_some(), cli.tls_cert_key_file.is_some());

            if let Some(tls_cert_file) = cli.tls_cert_file {
                if let Some(tls_cert_key_file) = cli.tls_cert_key_file {
                    config.listeners.iter_mut().for_each(|l| {
                        l.tls = Some(TLSConfig {
                            cert_file: tls_cert_file.clone(),
                            cert_key_file: tls_cert_key_file.clone(),
                        });
                    });
                }
            }
            // both config are required
            if (have_cert && !have_cert_key) || (!have_cert && have_cert_key) {
                return Err(Box::new(clap::error::Error::<clap::error::DefaultFormatter>::raw(
                    ErrorKind::ValueValidation,
                    "TLS certifcate file and certificate \
                        key file together are required",
                )));
            } else if !config.validate_nicknames() {
                Err(Box::new(clap::error::Error::<clap::error::DefaultFormatter>::raw(
                    ErrorKind::ValueValidation,
                    "Wrong nikname lengths",
                )))
            } else {
                Ok(config)
            }
        }
    }

    fn validate_nicknames(&self) -> bool {
        if let Some(ref users) = self.users {
            !users.iter().any(|u| u.nick.len() > 200)
        } else {
            true
        }
    }
}

impl Default for MainConfig {
    fn default() -> Self {
        MainConfig {
            name: "irc.irc".to_string(),
            admin_info: "ircadmin is IRC admin".to_string(),
            admin_info2: None,
            admin_email: None,
            info: "This is IRC server".to_string(),
            listeners: vec![ListenerConfig {
                listen: "127.0.0.1".parse().unwrap(),
                port: 6667,
                tls: None,
                websocket: false,
            }],
            network: "IRCnetwork".to_string(),
            password: None,
            motd: "Hello, world!".to_string(),
            max_connections: None,
            max_joins: None,
            ping_timeout: 120,
            pong_timeout: 20,
            dns_lookup: false,
            channels: None,
            operators: None,
            users: None,
            default_user_modes: UserModes {
                invisible: false,
                oper: false,
                local_oper: false,
                registered: false,
                wallops: false,
                websocket: false,
                secure: false,
                cloacked: false,
            },
            database: None,
            log_file: None,
            log_level: tracing::Level::INFO,
            #[cfg(feature = "amqp")]
            amqp: AmqpConfig {
                url: "amqp://guest:guest@localhost:5672/%2f".to_string(),
                exchange: "irc_exchange".to_string(),
                queue: "irc_queue".to_string(),
            },
            cloack: Cloacked {
                key1: "aaaaaaaaaabbbbbbbbbbbb".to_string(),
                key2: "bbbbbbbbbbbbbccccccccc".to_string(),
                key3: "cccccccccccccccccccfff".to_string(),
                prefix: "local-".to_string(),
            },
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::env::temp_dir;
    use std::fs;

    struct TempFileHandle {
        path: String,
    }

    impl TempFileHandle {
        fn new(path: &str) -> TempFileHandle {
            TempFileHandle {
                path: temp_dir().join(path).to_string_lossy().to_string(),
            }
        }
    }

    impl Drop for TempFileHandle {
        fn drop(&mut self) {
            fs::remove_file(self.path.as_str()).unwrap();
        }
    }

    #[test]
    fn test_usermodes_to_string() {
        assert_eq!(
            "+oOr".to_string(),
            UserModes {
                invisible: false,
                oper: true,
                local_oper: true,
                registered: true,
                wallops: false,
                websocket: false,
                secure: false,
                cloacked: false
            }
            .to_string()
        );
        assert_eq!(
            "+irw".to_string(),
            UserModes {
                invisible: true,
                oper: false,
                local_oper: false,
                registered: true,
                wallops: true,
                websocket: false,
                secure: false,
                cloacked: false
            }
            .to_string()
        );
        assert_eq!(
            "+Wz".to_string(),
            UserModes {
                invisible: false,
                oper: false,
                local_oper: false,
                registered: false,
                wallops: false,
                websocket: true,
                secure: true,
                cloacked: false
            }
            .to_string()
        );
    }

    #[test]
    fn test_channelmodes_to_string() {
        assert_eq!(
            "+itnl 10 +I somebody +o expert".to_string(),
            ChannelModes {
                ban: None,
                global_ban: None,
                exception: None,
                invite_exception: Some(["somebody".to_string()].into()),
                client_limit: Some(10),
                key: None,
                operators: Some(["expert".to_string()].into()),
                half_operators: None,
                voices: None,
                founders: None,
                protecteds: None,
                invite_only: true,
                moderated: false,
                secret: false,
                protected_topic: true,
                no_external_messages: true
            }
            .to_string()
        );
        let chm_str = ChannelModes {
            ban: Some(["somebody".to_string(), "somebody2".to_string()].into()),
            global_ban: None,
            exception: None,
            invite_exception: None,
            client_limit: None,
            key: Some("password".to_string()),
            operators: Some(["expert".to_string()].into()),
            half_operators: Some(["spec".to_string()].into()),
            voices: None,
            founders: None,
            protecteds: None,
            invite_only: false,
            moderated: false,
            secret: true,
            protected_topic: true,
            no_external_messages: false,
        }
        .to_string();
        assert!(
            "+stk password +b somebody +b somebody2 +o expert +h spec" == chm_str
                || "+stk password +b somebody2 +b somebody +o expert +h spec" == chm_str
        );
        let chm_str = ChannelModes {
            ban: None,
            global_ban: None,
            exception: None,
            invite_exception: Some(["somebody".to_string()].into()),
            client_limit: None,
            key: None,
            operators: None,
            half_operators: None,
            voices: Some(["guy1".to_string(), "guy2".to_string()].into()),
            founders: None,
            protecteds: None,
            invite_only: true,
            moderated: true,
            secret: false,
            protected_topic: false,
            no_external_messages: true,
        }
        .to_string();
        assert!(
            "+imn +I somebody +v guy1 +v guy2".to_string() == chm_str
                || "+imn +I somebody +v guy2 +v guy1".to_string() == chm_str
        );
        let chm_str = ChannelModes {
            ban: None,
            global_ban: None,
            exception: None,
            invite_exception: Some(["somebody".to_string()].into()),
            client_limit: None,
            key: None,
            operators: None,
            half_operators: None,
            founders: Some(["guy1".to_string(), "guy2".to_string()].into()),
            protecteds: None,
            voices: None,
            invite_only: true,
            moderated: true,
            secret: false,
            protected_topic: false,
            no_external_messages: true,
        }
        .to_string();
        assert!(
            "+imn +I somebody +q guy1 +q guy2".to_string() == chm_str
                || "+imn +I somebody +q guy2 +q guy1".to_string() == chm_str
        );
    }

    #[test]
    fn test_channelmodes_new_for_channel() {
        let mut exp_chm = ChannelModes::default();
        exp_chm.founders = Some(["biggy".to_string()].into());
        exp_chm.operators = Some(["biggy".to_string()].into());
        assert_eq!(exp_chm, ChannelModes::new_for_channel("biggy".to_string()));
    }

    #[test]
    fn test_channelmodes_banned() {
        let mut chm = ChannelModes::default();
        chm.ban = Some(["bom!*@*".to_string()].into());
        assert!(chm.banned("bom!bom@gugu.com"));
        assert!(chm.banned("bom!bam@ggregi.com"));
        assert!(!chm.banned("bam!bom@gugu.com"));
        chm.exception = Some(["bom!*@ggregi*".to_string()].into());
        assert!(chm.banned("bom!bom@gugu.com"));
        assert!(!chm.banned("bom!bam@ggregi.com"));
        chm.exception = Some(["*!*@ggregi*".to_string()].into());
        assert!(chm.banned("bom!bom@gugu.com"));
        assert!(!chm.banned("bom!bam@ggregi.com"));
        chm.ban = Some(["bom!*@*".to_string(), "zigi!*@*".to_string()].into());
        assert!(chm.banned("bom!bom@gugu.com"));
        assert!(chm.banned("zigi!zigol@gugu.com"));
        assert!(!chm.banned("bom!bam@ggregi.com"));
        assert!(!chm.banned("zigi!zigol@ggregi.net"));

        // Test global bans
        let mut chm = ChannelModes::default();
        chm.global_ban = Some(["bom!*@*".to_string()].into());
        assert!(chm.banned("bom!bom@gugu.com"));
        assert!(chm.banned("bom!bam@ggregi.com"));
        assert!(!chm.banned("bam!bom@gugu.com"));
        
        // Test both local and global bans
        chm.ban = Some(["zigi!*@*".to_string()].into());
        assert!(chm.banned("bom!bom@gugu.com")); // banned by global
        assert!(chm.banned("zigi!zigol@gugu.com")); // banned by local
        assert!(!chm.banned("other!user@host.com")); // not banned
        
        // Test exceptions with global bans
        chm.exception = Some(["bom!*@ggregi*".to_string()].into());
        assert!(chm.banned("bom!bom@gugu.com")); // still banned
        assert!(!chm.banned("bom!bam@ggregi.com")); // exception applies
    }

    #[test]
    fn test_channelmodes_rename_user() {
        let mut chm = ChannelModes::default();
        chm.operators = Some(["bobby".to_string(), "gugu".to_string()].into());
        chm.half_operators = Some(["bobby".to_string(), "alice".to_string()].into());
        chm.voices = Some(["bobby".to_string(), "nolan".to_string()].into());
        chm.founders = Some(["bobby".to_string(), "ben".to_string()].into());
        chm.protecteds = Some(["bobby".to_string(), "irek".to_string()].into());
        chm.rename_user(&"bobby".to_string(), "robert".to_string());
        let mut exp_chm = ChannelModes::default();
        exp_chm.operators = Some(["robert".to_string(), "gugu".to_string()].into());
        exp_chm.half_operators = Some(["robert".to_string(), "alice".to_string()].into());
        exp_chm.voices = Some(["robert".to_string(), "nolan".to_string()].into());
        exp_chm.founders = Some(["robert".to_string(), "ben".to_string()].into());
        exp_chm.protecteds = Some(["robert".to_string(), "irek".to_string()].into());
        assert_eq!(exp_chm, chm);
    }
}
