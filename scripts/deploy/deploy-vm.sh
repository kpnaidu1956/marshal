#!/bin/bash
# Full deployment script for goal-rag-server on a production VM
# Usage: ./deploy-vm.sh <domain>

set -e

DOMAIN="${1:-}"
INSTALL_DIR="/opt/goal-rag"
DATA_DIR="$INSTALL_DIR/data"
LOG_DIR="/var/log/goal-rag"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

if [[ -z "$DOMAIN" ]]; then
    echo "Usage: $0 <domain>"
    echo "Example: $0 rag.example.com"
    exit 1
fi

if [[ $EUID -ne 0 ]]; then
    log_error "Run as root: sudo $0 $DOMAIN"
    exit 1
fi

log_info "Starting deployment for $DOMAIN"

# 1. Create user and directories
log_info "Creating rag user and directories..."
id -u rag &>/dev/null || useradd -r -s /bin/false rag
mkdir -p $INSTALL_DIR $DATA_DIR $LOG_DIR
chown -R rag:rag $INSTALL_DIR $DATA_DIR $LOG_DIR

# 2. Copy binary (assumes it's in current directory or specify path)
if [[ -f "./goal-rag-server" ]]; then
    log_info "Installing binary..."
    cp ./goal-rag-server $INSTALL_DIR/
    chmod +x $INSTALL_DIR/goal-rag-server
    chown rag:rag $INSTALL_DIR/goal-rag-server
elif [[ -f "./target/release/goal-rag-server" ]]; then
    log_info "Installing binary from target/release..."
    cp ./target/release/goal-rag-server $INSTALL_DIR/
    chmod +x $INSTALL_DIR/goal-rag-server
    chown rag:rag $INSTALL_DIR/goal-rag-server
else
    log_warn "Binary not found. Build with: cargo build --release -p goal-rag"
    log_warn "Then copy to $INSTALL_DIR/goal-rag-server"
fi

# 3. Install systemd service
log_info "Installing systemd service..."
cat > /etc/systemd/system/goal-rag.service <<EOF
[Unit]
Description=Goal RAG Server
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=rag
Group=rag
WorkingDirectory=$INSTALL_DIR
ExecStart=$INSTALL_DIR/goal-rag-server
Restart=always
RestartSec=5

Environment="RUST_LOG=info"
Environment="RAG_HOST=127.0.0.1"
Environment="RAG_PORT=8080"
Environment="RAG_DATA_DIR=$DATA_DIR"

LimitNOFILE=65535
MemoryMax=4G

NoNewPrivileges=yes
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable goal-rag

# 4. Start RAG server
if [[ -f "$INSTALL_DIR/goal-rag-server" ]]; then
    log_info "Starting RAG server..."
    systemctl start goal-rag
    sleep 2

    # Check if running
    if systemctl is-active --quiet goal-rag; then
        log_info "RAG server is running"
    else
        log_error "RAG server failed to start"
        journalctl -u goal-rag -n 20
    fi
fi

# 5. Setup HTTPS with Caddy
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "$SCRIPT_DIR/setup-https.sh" ]]; then
    log_info "Setting up HTTPS..."
    bash "$SCRIPT_DIR/setup-https.sh" "$DOMAIN"
else
    log_warn "setup-https.sh not found. Run it separately."
fi

# 6. Summary
echo ""
echo "=========================================="
echo -e "${GREEN}Deployment Complete!${NC}"
echo "=========================================="
echo ""
echo "Services:"
echo "  - RAG Server: systemctl status goal-rag"
echo "  - Caddy:      systemctl status caddy"
echo ""
echo "Endpoints:"
echo "  - https://$DOMAIN/health"
echo "  - https://$DOMAIN/api/info"
echo "  - https://$DOMAIN/api/query"
echo ""
echo "Logs:"
echo "  - RAG:   journalctl -u goal-rag -f"
echo "  - Caddy: journalctl -u caddy -f"
echo ""
