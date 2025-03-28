#!/bin/bash

# Colores para mensajes
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

# Nombre del servicio
SERVICE_NAME="zeusircd2"
REPO_URL="https://github.com/Pryancito/zeusircd2.git"
INSTALL_DIR="/opt/zeusircd"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"

# Función para imprimir mensajes
print_message() {
    echo -e "${BLUE}[IRC-Server]${NC} $1"
}

# Función para verificar dependencias
check_dependencies() {
    print_message "Checking dependencies..."
    
    local deps=("git" "cargo" "rustc")
    local missing=()
    
    for dep in "${deps[@]}"; do
        if ! command -v "$dep" &> /dev/null; then
            missing+=("$dep")
        fi
    done
    
    if [ ${#missing[@]} -ne 0 ]; then
        echo -e "${RED}Error: Dependencies missing:${NC}"
        printf '%s\n' "${missing[@]}"
        exit 1
    fi
}

# Función para instalar el servidor
install_server() {
    print_message "Installing IRC Server..."
    
    # Crear directorio de instalación
    sudo mkdir -p "$INSTALL_DIR"
    
    # Clonar repositorio
    if [ ! -d "${INSTALL_DIR}/.git" ]; then
        sudo git clone "$REPO_URL" "$INSTALL_DIR"
    else
        print_message "The repository already exists, updating..."
        cd "$INSTALL_DIR" || exit
        sudo git pull
    fi
    
    # Compilar proyecto
    cd "$INSTALL_DIR" || exit
    sudo cargo build --release
    
    # Crear archivo de servicio systemd
    create_service_file

    print_message "Instalation finished"
}

# Función para crear archivo de servicio systemd
create_service_file() {
    print_message "Making service file ..."
    
    sudo tee "$SERVICE_FILE" > /dev/null << EOF
[Unit]
Description=Zeus IRC Server
After=network.target

[Service]
Type=simple
User=irc
ExecStart=${INSTALL_DIR}/target/release/zeusircd2 -c /opt/zeusircd2/ircd.toml
WorkingDirectory=${INSTALL_DIR}
Restart=always

[Install]
WantedBy=multi-user.target
EOF

    # Recargar systemd
    sudo systemctl daemon-reload
}

# Función para iniciar el servidor
start_server() {
    print_message "Starting IRC..."
    sudo systemctl start "$SERVICE_NAME"
}

# Función para detener el servidor
stop_server() {
    print_message "Stopping IRC..."
    sudo systemctl stop "$SERVICE_NAME"
}

# Función para actualizar desde GitHub
update_server() {
    print_message "Updating from GitHub..."
    stop_server
    cd "$INSTALL_DIR" || exit
    sudo git fetch
    sudo git pull
    sudo cargo build --release
    start_server
    print_message "Update completed"
}

# Función para mostrar el estado
show_status() {
    sudo systemctl status "$SERVICE_NAME"
}

# Menú principal
show_menu() {
    echo -e "\n${BLUE}=== IRC Server Manager ===${NC}"
    echo "1. Install server"
    echo "2. Start server"
    echo "3. Stop server"
    echo "4. Update from GitHub"
    echo "5. Show status"
    echo "6. Exit"
}

# Manejo de argumentos
case "$1" in
    "install")
        check_dependencies
        install_server
        ;;
    "start")
        start_server
        ;;
    "stop")
        stop_server
        ;;
    "update")
        update_server
        ;;
    "status")
        show_status
        ;;
    *)
        # Modo interactivo
        while true; do
            show_menu
            read -rp "Select one option: " option
            case $option in
                1) check_dependencies && install_server ;;
                2) start_server ;;
                3) stop_server ;;
                4) update_server ;;
                5) show_status ;;
                6) exit 0 ;;
                *) print_message "invalid option" ;;
            esac
        done
        ;;
esac 