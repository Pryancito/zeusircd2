// utils.rs - commands
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

use argon2::password_hash;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{self, Argon2};
use bytes::{BufMut, BytesMut};
use futures::task::{Context, Poll};
use futures::{Sink, SinkExt, Stream};
use lazy_static::lazy_static;
use std::convert::TryFrom;
use std::error::Error;
use std::io;
use std::pin::Pin;
use tokio::io::ReadBuf;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
#[cfg(feature = "tls")]
use tokio_openssl::SslStream;
use tokio_util::codec::{Decoder, Encoder, LinesCodec, LinesCodecError};
use validator::ValidationError;
use tokio_tungstenite::WebSocketStream;
use tungstenite::Message;
use tokio_util::codec::Framed;
use md5::{Md5, Digest};
use base64::{Engine as _, engine::general_purpose};

use crate::command::CommandError;
use crate::command::CommandError::*;
use crate::command::CommandId::*;

#[derive(Debug)]
pub(crate) enum DualTcpStream {
    PlainStream(TcpStream),
    #[cfg(feature = "tls")]
    SecureStream(SslStream<TcpStream>),
    WebSocketStream(WebSocketStream<TcpStream>),
    #[cfg(feature = "tls")]
    SecureWebSocketStream(WebSocketStream<SslStream<TcpStream>>),
}

impl DualTcpStream {
    pub(crate) fn is_secure(&self) -> bool {
        match self {
            DualTcpStream::PlainStream(_) => false,
            #[cfg(feature = "tls")]
            DualTcpStream::SecureStream(_) => true,
            DualTcpStream::WebSocketStream(_) => false,
            #[cfg(feature = "tls")]
            DualTcpStream::SecureWebSocketStream(_) => true,
        }
    }

    pub(crate) fn is_websocket(&self) -> bool {
        #[cfg(feature = "tls")]
        {
            matches!(self, DualTcpStream::WebSocketStream(_) | DualTcpStream::SecureWebSocketStream(_))
        }
        #[cfg(not(feature = "tls"))]
        {
            matches!(self, DualTcpStream::WebSocketStream(_))
        }
    }
}

impl AsyncRead for DualTcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            DualTcpStream::PlainStream(t) => Pin::new(t).poll_read(cx, buf),
            #[cfg(feature = "tls")]
            DualTcpStream::SecureStream(t) => Pin::new(t).poll_read(cx, buf),
            DualTcpStream::WebSocketStream(t) => {
                match Pin::new(t).poll_next(cx) {
                    Poll::Ready(Some(Ok(Message::Text(text)))) => {
                        buf.put_slice(text.as_bytes());
                        Poll::Ready(Ok(()))
                    }
                    Poll::Ready(Some(Ok(_))) => Poll::Ready(Ok(())),
                    Poll::Ready(Some(Err(e))) => Poll::Ready(Err(std::io::Error::other(e))),
                    Poll::Ready(None) => Poll::Ready(Ok(())),
                    Poll::Pending => Poll::Pending,
                }
            }
            #[cfg(feature = "tls")]
            DualTcpStream::SecureWebSocketStream(t) => {
                match Pin::new(t).poll_next(cx) {
                    Poll::Ready(Some(Ok(Message::Text(text)))) => {
                        buf.put_slice(text.as_bytes());
                        Poll::Ready(Ok(()))
                    }
                    Poll::Ready(Some(Ok(_))) => Poll::Ready(Ok(())),
                    Poll::Ready(Some(Err(e))) => Poll::Ready(Err(std::io::Error::other(e))),
                    Poll::Ready(None) => Poll::Ready(Ok(())),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

impl AsyncWrite for DualTcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.get_mut() {
            DualTcpStream::PlainStream(stream) => Pin::new(stream).poll_write(cx, buf),
            #[cfg(feature = "tls")]
            DualTcpStream::SecureStream(stream) => Pin::new(stream).poll_write(cx, buf),
            DualTcpStream::WebSocketStream(stream) => {
                match Pin::new(&mut *stream).poll_ready(cx) {
                    Poll::Ready(Ok(())) => {
                        match Pin::new(stream).start_send(Message::Text(String::from_utf8_lossy(buf).to_string().into())) {
                            Ok(()) => Poll::Ready(Ok(buf.len())),
                            Err(e) => Poll::Ready(Err(std::io::Error::other(e))),
                        }
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
                    Poll::Pending => Poll::Pending,
                }
            }
            #[cfg(feature = "tls")]
            DualTcpStream::SecureWebSocketStream(stream) => {
                match Pin::new(&mut *stream).poll_ready(cx) {
                    Poll::Ready(Ok(())) => {
                        match Pin::new(stream).start_send(Message::Text(String::from_utf8_lossy(buf).to_string().into())) {
                            Ok(()) => Poll::Ready(Ok(buf.len())),
                            Err(e) => Poll::Ready(Err(std::io::Error::other(e))),
                        }
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.get_mut() {
            DualTcpStream::PlainStream(stream) => Pin::new(stream).poll_flush(cx),
            #[cfg(feature = "tls")]
            DualTcpStream::SecureStream(stream) => Pin::new(stream).poll_flush(cx),
            DualTcpStream::WebSocketStream(stream) => {
                match Pin::new(stream).poll_flush(cx) {
                    Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
                    Poll::Pending => Poll::Pending,
                }
            }
            #[cfg(feature = "tls")]
            DualTcpStream::SecureWebSocketStream(stream) => {
                match Pin::new(stream).poll_flush(cx) {
                    Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.get_mut() {
            DualTcpStream::PlainStream(stream) => Pin::new(stream).poll_shutdown(cx),
            #[cfg(feature = "tls")]
            DualTcpStream::SecureStream(stream) => Pin::new(stream).poll_shutdown(cx),
            DualTcpStream::WebSocketStream(stream) => {
                match Pin::new(stream).poll_close(cx) {
                    Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
                    Poll::Pending => Poll::Pending,
                }
            }
            #[cfg(feature = "tls")]
            DualTcpStream::SecureWebSocketStream(stream) => {
                match Pin::new(stream).poll_close(cx) {
                    Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(std::io::Error::other(e))),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

// BufferedStream - to avoid deadlocks if no immediately data sent
#[derive(Debug)]
pub(crate) struct BufferedLineStream {
    stream: Framed<DualTcpStream, IRCLinesCodec>,
    buffer: Vec<String>,
}

impl BufferedLineStream {
    pub(crate) fn new(stream: DualTcpStream) -> Self {
        BufferedLineStream {
            stream: Framed::new(stream, IRCLinesCodec::new_with_max_length(2000)),
            buffer: vec![],
        }
    }

    pub(crate) async fn feed(&mut self, msg: String) -> Result<(), LinesCodecError> {
        self.buffer.push(msg);
        Ok(())
    }

    pub(crate) async fn flush(&mut self) -> Result<(), LinesCodecError> {
        for msg in self.buffer.drain(..) {
            self.stream.feed(msg).await?;
        }
        self.stream.flush().await?;
        Ok(())
    }

    pub(crate) fn is_secure(&self) -> bool {
        self.stream.get_ref().is_secure()
    }

    pub(crate) fn is_websocket(&self) -> bool {
        self.stream.get_ref().is_websocket()
    }
}

impl Stream for BufferedLineStream {
    type Item = Result<String, LinesCodecError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buf = [0u8; 1024];
        let mut read_buf = ReadBuf::new(&mut buf);
        match self.as_mut().get_mut().stream.get_mut() {
            DualTcpStream::PlainStream(_) => {
                Pin::new(&mut self.get_mut().stream).poll_next(cx)
            }
            #[cfg(feature = "tls")]
            DualTcpStream::SecureStream(_) => {
                Pin::new(&mut self.get_mut().stream).poll_next(cx)
            }
            DualTcpStream::WebSocketStream(_) => {
                match Pin::new(&mut self.get_mut().stream.get_mut()).poll_read(cx, &mut read_buf) {
                    Poll::Ready(Ok(())) => {
                        if read_buf.filled().is_empty() {
                            Poll::Ready(None)
                        } else {
                            let data = read_buf.filled().to_vec();
                            // Para WebSocket, simplemente convertimos a String
                            match String::from_utf8(data) {
                                Ok(s) => Poll::Ready(Some(Ok(s))),
                                Err(_) => Poll::Ready(Some(Err(LinesCodecError::Io(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "Invalid UTF-8 sequence",
                                ))))),
                            }
                        }
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Some(Err(LinesCodecError::Io(e)))),
                    Poll::Pending => Poll::Pending,
                }
            }
            #[cfg(feature = "tls")]
            DualTcpStream::SecureWebSocketStream(_) => {
                match Pin::new(&mut self.get_mut().stream.get_mut()).poll_read(cx, &mut read_buf) {
                    Poll::Ready(Ok(())) => {
                        if read_buf.filled().is_empty() {
                            Poll::Ready(None)
                        } else {
                            let data = read_buf.filled().to_vec();
                            // Para WebSocket, simplemente convertimos a String
                            match String::from_utf8(data) {
                                Ok(s) => Poll::Ready(Some(Ok(s))),
                                Err(_) => Poll::Ready(Some(Err(LinesCodecError::Io(io::Error::new(
                                    io::ErrorKind::InvalidData,
                                    "Invalid UTF-8 sequence",
                                ))))),
                            }
                        }
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Some(Err(LinesCodecError::Io(e)))),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, None)
    }
}

// special LinesCodec for IRC - encode with "\r\n".
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct IRCLinesCodec(LinesCodec);

impl IRCLinesCodec {
    pub(crate) fn new_with_max_length(max_length: usize) -> IRCLinesCodec {
        IRCLinesCodec(LinesCodec::new_with_max_length(max_length))
    }
}

impl Encoder<String> for IRCLinesCodec {
    type Error = LinesCodecError;

    fn encode(&mut self, line: String, buf: &mut BytesMut) -> Result<(), Self::Error> {
        buf.reserve(line.len() + 1);
        buf.put(line.as_bytes());
        buf.put_u8(b'\r');
        buf.put_u8(b'\n');
        Ok(())
    }
}

impl Decoder for IRCLinesCodec {
    type Item = String;
    type Error = LinesCodecError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<String>, Self::Error> {
        self.0.decode(buf)
    }
}

pub(crate) fn validate_username(username: &str) -> Result<(), ValidationError> {
    if !username.is_empty() && (username.as_bytes()[0] == b'#' || username.as_bytes()[0] == b'&') {
        Err(ValidationError::new(
            "Username must not have channel prefix.",
        ))
    } else if !username.contains('.') && !username.contains(':') && !username.contains(',') {
        Ok(())
    } else {
        Err(ValidationError::new(
            "Username must not contains '.', ',' or ':'.",
        ))
    }
}

pub(crate) fn validate_channel(channel: &str) -> Result<(), ValidationError> {
    if !channel.is_empty()
        && !channel.contains(':')
        && !channel.contains(',')
        && (channel.as_bytes()[0] == b'#' || channel.as_bytes()[0] == b'&')
    {
        Ok(())
    } else {
        Err(ValidationError::new(
            "Channel name must have '#' or '&' at start and \
                must not contains ',' or ':'.",
        ))
    }
}

pub(crate) fn validate_server<E: Error>(s: &str, e: E) -> Result<(), E> {
    if s.contains('.') {
        Ok(())
    } else {
        Err(e)
    }
}

pub(crate) fn validate_server_mask<E: Error>(s: &str, e: E) -> Result<(), E> {
    if s.contains('.') | s.contains('*') {
        Ok(())
    } else {
        Err(e)
    }
}

pub(crate) fn validate_prefixed_channel<E: Error>(channel: &str, e: E) -> Result<(), E> {
    if !channel.is_empty() && !channel.contains(':') && !channel.contains(',') {
        let mut is_channel = false;
        let mut last_amp = false;
        for (i, c) in channel.bytes().enumerate() {
            match c {
                b'~' | b'@' | b'%' | b'+' => (),
                b'&' => (),
                b'#' => {
                    is_channel = i + 1 < channel.len();
                    break;
                }
                _ => {
                    // if last special character is & - then local channel
                    is_channel = last_amp;
                    break;
                }
            }
            last_amp = c == b'&';
        }
        if is_channel {
            Ok(())
        } else {
            Err(e)
        }
    } else {
        Err(e)
    }
}

pub(crate) fn validate_usermodes<'a>(
    modes: &[(&'a str, Vec<&'a str>)],
) -> Result<(), CommandError> {
    let mut param_idx = 1;
    modes.iter().try_for_each(|(ms, margs)| {
        if !ms.is_empty() {
            if ms
                .find(|c| {
                    c != '+' && c != '-' && c != 'i' && c != 'o' && c != 'O' && c != 'r' && c != 'w' && c != 'W' && c != 'z' && c != 'x'
                })
                .is_some()
            {
                Err(UnknownUModeFlag(param_idx))
            } else if !margs.is_empty() {
                Err(WrongParameter(MODEId, param_idx))
            } else {
                param_idx += 1;
                Ok(())
            }
        } else {
            // if empty
            Err(WrongParameter(MODEId, param_idx))
        }
    })
}

pub(crate) fn validate_channelmodes<'a>(
    target: &'a str,
    modes: &[(&'a str, Vec<&'a str>)],
) -> Result<(), CommandError> {
    let mut param_idx = 1;
    modes.iter().try_for_each(|(ms, margs)| {
        if !ms.is_empty() {
            let mut mode_set = false;
            let mut arg_param_idx = param_idx + 1;

            let mut margs_it = margs.iter();

            ms.chars().try_for_each(|c| {
                match c {
                    '+' => {
                        mode_set = true;
                    }
                    '-' => {
                        mode_set = false;
                    }
                    'b' | 'B' | 'e' | 'I' => {
                        margs_it.next(); // consume argument
                        arg_param_idx += 1;
                    }
                    'o' | 'v' | 'h' | 'q' | 'a' => {
                        if let Some(arg) = margs_it.next() {
                            validate_username(arg).map_err(|e| InvalidModeParam {
                                target: target.to_string(),
                                modechar: c,
                                param: arg.to_string(),
                                description: e.to_string(),
                            })?;
                            arg_param_idx += 1;
                        } else {
                            return Err(InvalidModeParam {
                                target: target.to_string(),
                                modechar: c,
                                param: "".to_string(),
                                description: "No argument".to_string(),
                            });
                        }
                    }
                    'l' => {
                        if mode_set {
                            if let Some(arg) = margs_it.next() {
                                if let Err(e) = arg.parse::<usize>() {
                                    // if argument is not number, then error
                                    return Err(InvalidModeParam {
                                        target: target.to_string(),
                                        modechar: c,
                                        param: arg.to_string(),
                                        description: e.to_string(),
                                    });
                                }
                                arg_param_idx += 1;
                            } else {
                                return Err(InvalidModeParam {
                                    target: target.to_string(),
                                    modechar: c,
                                    param: "".to_string(),
                                    description: "No argument".to_string(),
                                });
                            }
                        } else if let Some(arg) = margs_it.next() {
                            return Err(InvalidModeParam {
                                target: target.to_string(),
                                modechar: c,
                                param: arg.to_string(),
                                description: "Unexpected argument".to_string(),
                            });
                        }
                    }
                    'k' => {
                        if mode_set {
                            if margs_it.next().is_some() {
                                arg_param_idx += 1;
                            } else {
                                // no argument
                                return Err(InvalidModeParam {
                                    target: target.to_string(),
                                    modechar: c,
                                    param: "".to_string(),
                                    description: "No argument".to_string(),
                                });
                            }
                        } else if let Some(arg) = margs_it.next() {
                            return Err(InvalidModeParam {
                                target: target.to_string(),
                                modechar: c,
                                param: arg.to_string(),
                                description: "Unexpected argument".to_string(),
                            });
                        }
                    }
                    'i' | 'm' | 't' | 'n' | 's' | 'r' => {}
                    c => {
                        return Err(UnknownMode(param_idx, c, target.to_string()));
                    }
                }
                Ok(())
            })?;

            param_idx += margs.len() + 1;

            Ok(())
        } else {
            // if empty
            Err(WrongParameter(MODEId, param_idx))
        }
    })
}

fn starts_single_wilcards<'a>(pattern: &'a str, text: &'a str) -> bool {
    if pattern.len() <= text.len() {
        pattern
            .bytes()
            .enumerate()
            .all(|(i, c)| c == b'?' || c == text.as_bytes()[i])
    } else {
        false
    }
}

pub(crate) fn match_wildcard<'a>(pattern: &'a str, text: &'a str) -> bool {
    let mut pat = pattern;
    let mut t = text;
    let mut asterisk = false;
    while !pat.is_empty() {
        let (newpat, m, cur_ast) = if let Some(i) = pat.find('*') {
            (&pat[i + 1..], &pat[..i], true)
        } else {
            (&pat[pat.len()..pat.len()], pat, false)
        };

        if !m.is_empty() {
            if !asterisk {
                // if first match
                if !starts_single_wilcards(m, t) {
                    return false;
                }
                t = &t[m.len()..];
            } else if cur_ast || !newpat.is_empty() {
                // after asterisk. only if some rest in pattern and
                // if last current character is asterisk
                let mut i = 0;
                // find first single wildcards occurrence.
                while i <= t.len() - m.len() && !starts_single_wilcards(m, &t[i..]) {
                    i += 1;
                }
                if i <= t.len() - m.len() {
                    // if found
                    t = &t[i + m.len()..];
                } else {
                    return false;
                }
            } else {
                // if last pattern is not asterisk
                if !starts_single_wilcards(m, &t[t.len() - m.len()..]) {
                    return false;
                }
                t = &t[t.len()..t.len()];
            }
        }

        asterisk = true;
        pat = newpat;
    }
    // if last character in pattern is '*' or text has been fully consumed
    (!pattern.is_empty() && pattern.as_bytes()[pattern.len() - 1] == b'*') || t.is_empty()
}

// normalize source mask - for example '*' to '*!*@*'
pub(crate) fn normalize_sourcemask(mask: &str) -> String {
    let mut out = String::new();
    if let Some(p) = mask.find('!') {
        out += mask; // normalized
        if mask[p + 1..].find('@').is_none() {
            out += "@*";
        }
    } else if let Some(p2) = mask.find('@') {
        out += &mask[..p2];
        out += "!*";
        out += &mask[p2..];
    } else {
        out += mask; // normalized
        out += "!*@*";
    }
    out
}

//  argon2

static ARGON2_M_COST: u32 = 2048;
static ARGON2_T_COST: u32 = 2;
static ARGON2_P_COST: u32 = 1;
static ARGON2_OUT_LEN: usize = 64;

lazy_static! {
    static ref ARGON2_SALT: SaltString = SaltString::b64_encode(
        option_env!("PASSWORD_SALT")
            .unwrap_or("br8f4efc3F4heecdsdS")
            .as_bytes()
    )
    .unwrap();
    static ref ARGON2: Argon2<'static> = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(
            ARGON2_M_COST,
            ARGON2_T_COST,
            ARGON2_P_COST,
            Some(ARGON2_OUT_LEN)
        )
        .unwrap()
    );
}

pub(crate) fn argon2_hash_password(password: &str) -> String {
    ARGON2
        .hash_password(password.as_bytes(), ARGON2_SALT.as_str())
        .unwrap()
        .hash
        .unwrap()
        .to_string()
}

pub(crate) fn argon2_verify_password<'a>(
    password: &'a str,
    hash_str: &'a str,
) -> password_hash::errors::Result<()> {
    let password_hash = PasswordHash {
        algorithm: argon2::Algorithm::Argon2id.ident(),
        version: Some(argon2::Version::V0x13.into()),
        params: password_hash::ParamsString::try_from(ARGON2.params()).unwrap(),
        salt: Some(ARGON2_SALT.as_salt()),
        hash: Some(password_hash::Output::b64_decode(hash_str)?),
    };
    ARGON2.verify_password(password.as_bytes(), &password_hash)
}

pub(crate) async fn argon2_verify_password_async(
    password: String,
    hash_str: String,
) -> password_hash::errors::Result<()> {
    tokio::task::spawn_blocking(move || argon2_verify_password(&password, &hash_str))
        .await
        .unwrap()
}

pub(crate) fn validate_password_hash(hash_str: &str) -> Result<(), ValidationError> {
    match password_hash::Output::b64_decode(hash_str) {
        Ok(o) => {
            if o.len() == ARGON2_OUT_LEN {
                Ok(())
            } else {
                Err(ValidationError::new("Wrong password hash length"))
            }
        }
        Err(_) => Err(ValidationError::new("Wrong base64 password hash")),
    }
}

/// Verifica la autenticación SASL PLAIN
pub(crate) async fn verify_sasl_plain(
    data: &str,
    config: &crate::config::MainConfig,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    // Decodificar datos base64
    let decoded = match general_purpose::STANDARD.decode(data) {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };
    
    // Convertir a string
    let auth_string = match String::from_utf8(decoded) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    
    // Parsear el formato: authorization_id\0authentication_id\0password
    let parts: Vec<&str> = auth_string.split('\0').collect();
    if parts.len() != 3 {
        return Ok(None);
    }
    
    let _authz_id = parts[0]; // authorization_id (ignorado por ahora)
    let authn_id = parts[1];  // authentication_id (username/nick)
    let password = parts[2];  // password
    
    // Verificar contra la configuración de usuarios
    if let Some(ref users) = config.users {
        for user in users {
            if user.name == authn_id {
                if let Some(ref user_password) = user.password {
                    // Verificar contraseña con Argon2
                    if argon2_verify_password_async(password.to_string(), user_password.clone())
                        .await
                        .is_ok()
                    {
                        return Ok(Some(authn_id.to_string()));
                    }
                }
            }
        }
    }
    
    // Verificar contra contraseña por defecto
    if let Some(ref default_password) = config.password {
        if argon2_verify_password_async(password.to_string(), default_password.clone())
            .await
            .is_ok()
        {
            return Ok(Some(authn_id.to_string()));
        }
    }
    
    // Si no hay usuarios configurados ni contraseña por defecto,
    // retornar el username para que se verifique contra NickServ
    if config.users.is_none() && config.password.is_none() {
        return Ok(Some(authn_id.to_string()));
    }
    
    Ok(None)
}

/// Verifica la autenticación SASL MD5
pub(crate) async fn verify_sasl_md5(
    data: &str,
    config: &crate::config::MainConfig,
) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
    // Decodificar datos base64
    let decoded = match general_purpose::STANDARD.decode(data) {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };
    
    // Convertir a string
    let auth_string = match String::from_utf8(decoded) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    
    // Parsear el formato: username\0password
    let parts: Vec<&str> = auth_string.split('\0').collect();
    if parts.len() != 2 {
        return Ok(None);
    }
    
    let username = parts[0];
    let password = parts[1];
    
    // Verificar contra la configuración de usuarios
    if let Some(ref users) = config.users {
        for user in users {
            if user.name == username {
                if let Some(ref user_password) = user.password {
                    // Para MD5, asumimos que el hash almacenado es MD5
                    // Calcular hash MD5 de la contraseña proporcionada
                    let mut hasher = Md5::new();
                    hasher.update(password.as_bytes());
                    let password_hash = format!("{:x}", hasher.finalize());
                    
                    // Comparar con el hash almacenado
                    if password_hash == *user_password {
                        return Ok(Some(username.to_string()));
                    }
                }
            }
        }
    }
    
    // Verificar contra contraseña por defecto
    if let Some(ref default_password) = config.password {
        // Calcular hash MD5 de la contraseña proporcionada
        let mut hasher = Md5::new();
        hasher.update(password.as_bytes());
        let password_hash = format!("{:x}", hasher.finalize());
        
        if password_hash == *default_password {
            return Ok(Some(username.to_string()));
        }
    }
    
    Ok(None)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_irc_lines_codec() {
        let mut codec = IRCLinesCodec::new_with_max_length(2000);
        let mut buf = BytesMut::new();
        codec.encode("my line".to_string(), &mut buf).unwrap();
        assert_eq!("my line\r\n".as_bytes(), buf);
        let mut buf = BytesMut::from("my line 2\n");
        assert_eq!(
            codec.decode(&mut buf).map_err(|e| e.to_string()),
            Ok(Some("my line 2".to_string()))
        );
        assert_eq!(buf, BytesMut::new());
        let mut buf = BytesMut::from("my line 2\r\n");
        assert_eq!(
            codec.decode(&mut buf).map_err(|e| e.to_string()),
            Ok(Some("my line 2".to_string()))
        );
        assert_eq!(buf, BytesMut::new());
    }

    #[test]
    fn test_validate_source() {
        assert_eq!(true, validate_source("bob!bobby@host.com"));
        assert_eq!(true, validate_source("bobby@host.com"));
        assert_eq!(true, validate_source("bob!bobby"));
        assert_eq!(true, validate_source("host.com"));
        assert_eq!(false, validate_source("bob@bobby!host.com"));
    }

    #[test]
    fn test_validate_username() {
        assert_eq!(true, validate_username("ala").is_ok());
        assert_eq!(false, validate_username("#ala").is_ok());
        assert_eq!(false, validate_username("&ala").is_ok());
        assert_eq!(false, validate_username("a.la").is_ok());
        assert_eq!(false, validate_username("a,la").is_ok());
        assert_eq!(false, validate_username("aL:a").is_ok());
    }

    #[test]
    fn test_validate_channel() {
        assert_eq!(true, validate_channel("#ala").is_ok());
        assert_eq!(true, validate_channel("&ala").is_ok());
        assert_eq!(false, validate_channel("&al:a").is_ok());
        assert_eq!(false, validate_channel("&al,a").is_ok());
        assert_eq!(false, validate_channel("#al:a").is_ok());
        assert_eq!(false, validate_channel("#al,a").is_ok());
        assert_eq!(false, validate_channel("ala").is_ok());
    }

    #[test]
    fn test_validate_server() {
        assert_eq!(
            true,
            validate_server("somebody.org", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_server("somebodyorg", WrongParameter(PINGId, 0)).is_ok()
        );
    }

    #[test]
    fn test_validate_server_mask() {
        assert_eq!(
            true,
            validate_server_mask("somebody.org", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_server_mask("*org", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_server_mask("somebodyorg", WrongParameter(PINGId, 0)).is_ok()
        );
    }

    #[test]
    fn test_validate_prefixed_channel() {
        assert_eq!(
            true,
            validate_prefixed_channel("#ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("&ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_prefixed_channel("&al:a", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_prefixed_channel("&al,a", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_prefixed_channel("#al:a", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_prefixed_channel("#al,a", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_prefixed_channel("ala", WrongParameter(PINGId, 0)).is_ok()
        );

        assert_eq!(
            true,
            validate_prefixed_channel("~#ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("+#ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("%#ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("&#ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("@#ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("~&ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("+&ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("%&ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("&&ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            true,
            validate_prefixed_channel("@&ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_prefixed_channel("*#ala", WrongParameter(PINGId, 0)).is_ok()
        );
        assert_eq!(
            false,
            validate_prefixed_channel("*&ala", WrongParameter(PINGId, 0)).is_ok()
        );
    }

    #[test]
    fn test_validate_usermodes() {
        assert_eq!(
            Ok(()),
            validate_usermodes(&vec![("+io-rw", vec![]), ("-O", vec![])])
                .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_usermodes(&vec![("+io", vec![]), ("-rO", vec![]), ("-w", vec![])])
                .map_err(|e| e.to_string())
        );
        assert_eq!(
            Err("Wrong parameter 1 in command 'MODE'".to_string()),
            validate_usermodes(&vec![("+io-rw", vec!["xx"]), ("-O", vec![])])
                .map_err(|e| e.to_string())
        );
        assert_eq!(
            Err("Unknown umode flag in parameter 2".to_string()),
            validate_usermodes(&vec![(" +io-rw", vec![]), ("-x", vec![])])
                .map_err(|e| e.to_string())
        );
    }

    #[test]
    fn test_validate_channelmodes() {
        assert_eq!(
            Ok(()),
            validate_channelmodes("#xchan", &vec![("+nt", vec![]), ("-sm", vec![])])
                .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_channelmodes(
                "#xchan",
                &vec![("+nlt", vec!["22"]), ("-s+km", vec!["xxyy"])]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_channelmodes(
                "#xchan",
                &vec![("+ibl-h", vec!["*dudu.com", "22", "derek"])]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_channelmodes("#xchan", &vec![("-nlt", vec![]), ("+s-km", vec![])])
                .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_channelmodes(
                "#xchan",
                &vec![
                    ("+ot", vec!["barry"]),
                    ("-nh", vec!["guru"]),
                    ("+vm", vec!["jerry"])
                ]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_channelmodes(
                "#xchan",
                &vec![
                    ("-to", vec!["barry"]),
                    ("+hn", vec!["guru"]),
                    ("-mv", vec!["jerry"])
                ]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_channelmodes(
                "#xchan",
                &vec![
                    ("-tb", vec!["barry"]),
                    ("+iI", vec!["guru"]),
                    ("-es", vec!["eagle"])
                ]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_channelmodes(
                "#xchan",
                &vec![
                    ("+tb", vec!["barry"]),
                    ("-iI", vec!["guru"]),
                    ("+es", vec!["eagle"])
                ]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Ok(()),
            validate_channelmodes(
                "#xchan",
                &vec![
                    ("-to", vec!["barry"]),
                    ("+an", vec!["guru"]),
                    ("-mq", vec!["jerry"])
                ]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Err("Unknown mode u in parameter 2 for #xchan".to_string()),
            validate_channelmodes("#xchan", &vec![("+nt", vec![]), ("-sum", vec![])])
                .map_err(|e| e.to_string())
        );
        assert_eq!(
            Err("Invalid mode parameter: #xchan l  No argument".to_string()),
            validate_channelmodes("#xchan", &vec![("+nlt", vec![]), ("-s+km", vec!["xxyy"])])
                .map_err(|e| e.to_string())
        );
        assert_eq!(
            Err(
                "Invalid mode parameter: #xchan v jer:ry Validation error: Username \
                must not contains '.', ',' or ':'. [{}]"
                    .to_string()
            ),
            validate_channelmodes(
                "#xchan",
                &vec![
                    ("+ot", vec!["barry"]),
                    ("-nh", vec!["guru"]),
                    ("+vm", vec!["jer:ry"])
                ]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Err(
                "Invalid mode parameter: #xchan h gu:ru Validation error: Username \
                must not contains '.', ',' or ':'. [{}]"
                    .to_string()
            ),
            validate_channelmodes(
                "#xchan",
                &vec![
                    ("+ot", vec!["barry"]),
                    ("-nh", vec!["gu:ru"]),
                    ("+vm", vec!["jerry"])
                ]
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Err(
                "Invalid mode parameter: #xchan o b,arry Validation error: Username \
                must not contains '.', ',' or ':'. [{}]"
                    .to_string()
            ),
            validate_channelmodes(
                "#xchan",
                &vec![
                    ("+ot", vec!["b,arry"]),
                    ("-nh", vec!["guru"]),
                    ("+vm", vec!["jerry"])
                ]
            )
            .map_err(|e| e.to_string())
        );
    }

    #[test]
    fn test_match_wildcard() {
        assert!(match_wildcard("somebody", "somebody"));
        assert!(!match_wildcard("somebody", "somebady"));
        assert!(match_wildcard("s?meb?dy", "samebady"));
        assert!(!match_wildcard("s?mec?dy", "samebady"));
        assert!(!match_wildcard("somebody", "somebod"));
        assert!(!match_wildcard("somebody", "somebodyis"));
        assert!(match_wildcard("so*body", "somebody"));
        assert!(match_wildcard("so**body", "somebody"));
        assert!(match_wildcard("so*body", "sobody"));
        assert!(match_wildcard("so*body*", "sobody"));
        assert!(match_wildcard("*so*body*", "sobody"));
        assert!(!match_wildcard("so*body", "sbody"));
        assert!(!match_wildcard("*so*body*", "sbody"));
        assert!(match_wildcard("so*body", "something body"));
        assert!(match_wildcard("so*bo*", "somebody"));
        assert!(match_wildcard("*", "Alice and Others"));
        assert!(!match_wildcard("", "Alice and Others"));
        assert!(match_wildcard("", ""));
        assert!(match_wildcard("*", ""));
        assert!(match_wildcard("***", ""));
        assert!(match_wildcard("* and Others", "Alice and Others"));
        assert!(!match_wildcard("* and Others", "Alice and others"));
        assert!(!match_wildcard("* and Others", "Aliceand Others"));
        assert!(match_wildcard("* and *", "Alice and Others"));
        assert!(match_wildcard("*** and **", "Alice and Others"));
        assert!(!match_wildcard("* and *", "Aliceand Others"));
        assert!(!match_wildcard("* and *", "Alice andOthers"));
        assert!(!match_wildcard("*** and ***", "Aliceand Others"));
        assert!(!match_wildcard("*** and ***", "Alice andOthers"));
        assert!(match_wildcard("*?and *", "Aliceand Others"));
        assert!(match_wildcard("* and?*", "Alice andOthers"));
        assert!(!match_wildcard("*?and *", "Aliceund Others"));
        assert!(!match_wildcard("* and?*", "Alice undOthers"));
        assert!(match_wildcard("lu*na*Xna*Y", "lulu and nanaXnaY"));
        assert!(match_wildcard(
            "lu*Xlu*Wlu*Zlu*B",
            "lulululuYlululuXlululuWluluZluluAluluB"
        ));
        assert!(match_wildcard(
            "lu*?lu*?lu*?lu*?",
            "lulululuYlululuXlululuWluluZluluAluluB"
        ));
        assert!(match_wildcard(
            "*lu*Xlu*Wlu*Zlu*B*",
            "XXXlulululuYlululuXlululuWluluZluluAluluBlululu"
        ));
        assert!(match_wildcard("la*la", "labulabela"));
        assert!(!match_wildcard("la*la", "labulabele"));
        assert!(match_wildcard("la*la*la", "labulalabela"));
        assert!(!match_wildcard("la*la*la", "labulalabele"));
        assert!(match_wildcard("la*l?", "labulabela"));
        assert!(!match_wildcard("la*?a", "labulabele"));
        assert!(!match_wildcard("la*l?", "labulabeka"));
        assert!(match_wildcard("greg*@somehere*", "greg-guru@somehere.net"));
        assert!(match_wildcard("greg*@somehere*", "greg@@@@somehere@@@"));
        assert!(!match_wildcard("greg*@somehere*", "greg.somehere@@@"));
    }

    #[test]
    fn test_normalize_sourcemask() {
        assert_eq!("ax*!*bob*@*.com", &normalize_sourcemask("ax*!*bob*@*.com"));
        assert_eq!("ax*!*@*.com", &normalize_sourcemask("ax*@*.com"));
        assert_eq!("ax*!bo*@*", &normalize_sourcemask("ax*!bo*"));
        assert_eq!("*ax!*@*.com", &normalize_sourcemask("*ax@*.com"));
        assert_eq!("u*xn!b*o@*", &normalize_sourcemask("u*xn!b*o"));
        assert_eq!("*!*@*", &normalize_sourcemask("*"));
        assert_eq!("bob.com!*@*", &normalize_sourcemask("bob.com"));
    }

    #[test]
    fn test_test_argon2_verify_password() {
        let phash = argon2_hash_password("lalalaXX");
        assert!(argon2_verify_password("lalalaXX", &phash).is_ok());
        assert!(argon2_verify_password("lalalaXY", &phash).is_err());
    }

    #[tokio::test]
    async fn test_argon2_verify_password_async() {
        let phash = argon2_hash_password("lalalaXX");
        assert!(
            argon2_verify_password_async("lalalaXX".to_string(), phash.clone())
                .await
                .is_ok()
        );
        assert!(argon2_verify_password_async("lalalaXY".to_string(), phash)
            .await
            .is_err());
    }

    #[test]
    fn test_validate_password_hash() {
        assert!(validate_password_hash(
            "VgWezXctjWvsY6V7gzSQPnluUuAwq06m5IxwcIg3OfBIMM+zWCJ\
        ntk8HEZDgh4ctFei3bqt1r0O1VIyOV7dL+w"
        )
        .is_ok());
        assert_eq!(
            Err("Validation error: Wrong password hash length [{}]".to_string()),
            validate_password_hash(
                "zXctjWvsY6V7gzSQPnluUuAwq06m5IxwcIg3OfBIMM+zWCJ\
                ntk8HEZDgh4ctFei3bqt1r0O1VIyOV7dL+w"
            )
            .map_err(|e| e.to_string())
        );
        assert_eq!(
            Err("Validation error: Wrong base64 password hash [{}]".to_string()),
            validate_password_hash("xxxxxxxxx").map_err(|e| e.to_string())
        );
    }
}
