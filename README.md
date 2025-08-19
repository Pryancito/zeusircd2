# ZeusiRCd2 IRC Server

[![LGPL 2.1 License](https://img.shields.io/badge/License-LGPL--2.1-brightgreen)](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html)

ZeusiRCd2 is a modern and efficient IRC server written in Rust. Designed with an asynchronous approach and modular architecture, it provides a solid foundation for distributed IRC networks.

## 🚀 Main Features

### **Architecture and Performance**
- **Asynchronous design** based on the Tokio framework
- **Modular architecture** with interchangeable components
- **High performance** with efficient handling of concurrent connections
- **Low memory usage** optimized for production environments

### **Connectivity and Protocols**
- **TLS/SSL connections** for secure communication
- **AMQP support** for communication between distributed servers
- **Integrated DNS resolution** for host lookups
- **WebSocket** for modern web clients

### **IRC Functionality**
- **Complete standard IRC commands** (PRIVMSG, NOTICE, MODE, etc.)
- **User modes** and **channel modes** configurable
- **Operator system** with granular privileges
- **Channel management** with support for bans and timeouts
- **Integrated services** (NickServ, ChanServ)

### **Security and Authentication**
- **Password hashing** with Argon2
- **Robust authentication system**
- **Configurable host cloaking**
- **Protection against common attacks**

### **Integration and Services**
- **MySQL and SQLite databases**
- **REST API** for administration
- **Advanced logging** with configurable levels
- **Connection and event monitoring**

## 🛠️ Compilation

### **Requirements**
- Rust 1.70+
- Cargo (included with Rust)
- OpenSSL (for TLS)
- MySQL/SQLite (optional)

### **Available Features**

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| `dns_lookup` | DNS resolution with Trust DNS | - |
| `tls` | TLS/SSL connections | OpenSSL |
| `amqp` | Inter-server communication | RabbitMQ |
| `mysql` | MySQL database | MySQL |
| `sqlite` | SQLite database | SQLite |

### **Compilation Commands**

**Complete compilation with all features:**
```bash
cargo build --release --features=dns_lookup,tls,amqp,mysql,sqlite
```

**Basic compilation (without DNS or TLS):**
```bash
cargo build --release
```

**Compilation with AMQP for distributed servers:**
```bash
cargo build --release --features=amqp
```

### **Environment Variables for Security**
```bash
export PASSWORD_SALT="your_custom_salt"
cargo build --release
```

## ⚙️ Configuration

### **Configuration File**
The server uses TOML files for configuration. Basic example:

```toml
[server]
name = "irc.example.com"
bind = "127.0.0.1"
ports = [6667, 6697]
tls_ports = [6697]

[amqp]
url = "amqp://localhost:5672"
exchange = "irc_events"
queue = "server_messages"

[services]
mysql_url = "mysql://user:pass@localhost/zeusircd2"
```

### **Password Hash Generation**
```bash
# Generate interactive hash
zeusircd2 -g

# Generate hash with specific password
zeusircd2 -g -P "my_password"
```

## 🚀 Execution

### **Basic Command**
```bash
zeusircd2 -c config.toml
```

### **Command Line Options**
```bash
zeusircd2 -c config.toml --log-level debug
zeusircd2 -c config.toml --daemon
```

## 🔗 Inter-Server Communication (AMQP)

### **AMQP Features**
- **Automatic monitoring** of connections/disconnections
- **User synchronization** between servers
- **Message broadcasting** to entire network
- **Distributed server management**

### **AMQP Configuration**
```toml
[amqp]
url = "amqp://localhost:5672"
exchange = "irc_events"
queue = "server_messages"
server_name = "irc1.example.com"
```

## **AMQP Server**
To activate server visualization, you must
enter this command on your RabbitMQ server:

```bash
sudo rabbitmq-plugins enable rabbitmq_event_exchange
```

### **Detected Events**
- ✅ New server connections
- ✅ Server disconnections
- ✅ User synchronization
- ✅ Message broadcasting

## 📊 Monitoring and Logs

### **Log Levels**
- `error`: Critical errors
- `warn`: Warnings
- `info`: General information
- `debug`: Detailed information
- `trace`: Complete tracing

### **AMQP Log Examples**
```
INFO --> New connection detected! IRC Server: 'irc2.localhost' (v0.0.9) UUID: 84ea8998-17ab-48bb-a41b-eaeff4b529d7
INFO --> Connection closed detected! IRC Server: 'irc2.localhost' UUID: 21c659ae-c442-47e6-add9-a127e4128781
```

## 🗄️ Databases

### **MySQL**
```toml
[db]
url = "mysql://user:password@localhost/zeusircd2"
```

### **SQLite**
```toml
[db]
url = "/path/to/zeusircd2.db"
```

## 🔧 Development

### **Tests**
```bash
cargo test
cargo test --features=amqp
```

## 📝 License

This project is licensed under LGPL 2.1. See [COPYING](COPYING) for more details.

## 🤝 Contributions

Contributions are welcome. Please:

1. Fork the repository
2. Create a branch for your feature
3. Commit your changes
4. Push to the branch
5. Open a Pull Request

## 📞 Support

To report bugs or request features, please use GitHub issues.

---

**Note**: This server is designed for simple and local environments. For production use, proper security and monitoring configuration is recommended.
