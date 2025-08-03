# ZeusiRCd2 IRC Server

[![LGPL 2.1 License](https://img.shields.io/badge/License-LGPL--2.1-brightgreen)](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html)

ZeusiRCd2 es un servidor IRC moderno y eficiente escrito en Rust. Diseñado con un enfoque asíncrono y arquitectura modular, proporciona una base sólida para redes IRC distribuidas.

## 🚀 Características Principales

### **Arquitectura y Rendimiento**
- **Diseño asíncrono** basado en el framework Tokio
- **Arquitectura modular** con componentes intercambiables
- **Alto rendimiento** con manejo eficiente de conexiones concurrentes
- **Bajo uso de memoria** optimizado para entornos de producción

### **Conectividad y Protocolos**
- **Conexiones TLS/SSL** para comunicación segura
- **Soporte AMQP** para comunicación entre servidores distribuidos
- **Resolución DNS** integrada para lookup de hosts
- **WebSocket** para clientes web modernos

### **Funcionalidades IRC**
- **Comandos IRC estándar** completos (PRIVMSG, NOTICE, MODE, etc.)
- **Modos de usuario** y **modos de canal** configurables
- **Sistema de operadores** con privilegios granulares
- **Gestión de canales** con soporte para bans y timeouts
- **Servicios integrados** (NickServ, ChanServ)

### **Seguridad y Autenticación**
- **Hashing de contraseñas** con Argon2
- **Sistema de autenticación** robusto
- **Cloaking de hosts** configurable
- **Protección contra ataques** comunes

### **Integración y Servicios**
- **Bases de datos** MySQL y SQLite
- **API REST** para administración
- **Logging avanzado** con niveles configurables
- **Monitoreo** de conexiones y eventos

## 🛠️ Compilación

### **Requisitos**
- Rust 1.70+ 
- Cargo (incluido con Rust)
- OpenSSL (para TLS)
- MySQL/SQLite (opcional)

### **Features Disponibles**

| Feature | Descripción | Dependencias |
|---------|-------------|--------------|
| `dns_lookup` | Resolución DNS con Trust DNS | - |
| `tls` | Conexiones TLS/SSL | OpenSSL |
| `amqp` | Comunicación entre servidores | RabbitMQ |
| `mysql` | Base de datos MySQL | MySQL |
| `sqlite` | Base de datos SQLite | SQLite |

### **Comandos de Compilación**

**Compilación completa con todas las features:**
```bash
cargo build --release --features=dns_lookup,tls,amqp,mysql,sqlite
```

**Compilación básica (sin DNS ni TLS):**
```bash
cargo build --release
```

**Compilación con AMQP para servidores distribuidos:**
```bash
cargo build --release --features=amqp
```

### **Variables de Entorno para Seguridad**
```bash
export PASSWORD_SALT="tu_salt_personalizado"
cargo build --release
```

## ⚙️ Configuración

### **Archivo de Configuración**
El servidor usa archivos TOML para configuración. Ejemplo básico:

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

### **Generación de Hashes de Contraseña**
```bash
# Generar hash interactivo
zeusircd2 -g

# Generar hash con contraseña específica
zeusircd2 -g -P "mi_contraseña"
```

## 🚀 Ejecución

### **Comando Básico**
```bash
zeusircd2 -c config.toml
```

### **Opciones de Línea de Comandos**
```bash
zeusircd2 -c config.toml --log-level debug
zeusircd2 -c config.toml --daemon
```

## 🔗 Comunicación entre Servidores (AMQP)

### **Características AMQP**
- **Monitoreo automático** de conexiones/desconexiones
- **Sincronización de usuarios** entre servidores
- **Broadcasting de mensajes** a toda la red
- **Gestión de servidores** distribuidos

### **Configuración AMQP**
```toml
[amqp]
url = "amqp://localhost:5672"
exchange = "irc_events"
queue = "server_messages"
server_name = "irc1.example.com"
```

## **Servidor AMQP**
Para activar la visualización de servidores deberás
introducir este comando en tu servidor de rabbitmq:

```bash
sudo rabbitmq-plugins enable rabbitmq_event_exchange
```

### **Eventos Detectados**
- ✅ Conexiones de nuevos servidores
- ✅ Desconexiones de servidores
- ✅ Sincronización de usuarios
- ✅ Broadcast de mensajes

## 📊 Monitoreo y Logs

### **Niveles de Log**
- `error`: Errores críticos
- `warn`: Advertencias
- `info`: Información general
- `debug`: Información detallada
- `trace`: Trazado completo

### **Ejemplo de Logs AMQP**
```
INFO --> ¡Nueva conexión detectada! Servidor IRC: 'irc2.localhost' (v0.0.9) UUID: 84ea8998-17ab-48bb-a41b-eaeff4b529d7
INFO --> ¡Conexión cerrada detectada! Servidor IRC: 'irc2.localhost' UUID: 21c659ae-c442-47e6-add9-a127e4128781
```

## 🗄️ Bases de Datos

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

## 🔧 Desarrollo

### **Tests**
```bash
cargo test
cargo test --features=amqp
```

## 📝 Licencia

Este proyecto está licenciado bajo LGPL 2.1. Ver [COPYING](COPYING) para más detalles.

## 🤝 Contribuciones

Las contribuciones son bienvenidas. Por favor:

1. Fork el repositorio
2. Crea una rama para tu feature
3. Commit tus cambios
4. Push a la rama
5. Abre un Pull Request

## 📞 Soporte

Para reportar bugs o solicitar features, por favor usa los issues de GitHub.

---

**Nota**: Este servidor está diseñado para entornos simples y locales. Para uso en producción, se recomienda configurar adecuadamente la seguridad y el monitoreo.
