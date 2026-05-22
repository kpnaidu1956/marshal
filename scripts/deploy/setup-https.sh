#!/bin/bash
# Setup HTTPS for ruvector-rag-server using Caddy reverse proxy
# Usage: ./setup-https.sh <domain>
# Example: ./setup-https.sh rag.example.com

set -e

DOMAIN="${1:-}"
RAG_PORT="${RAG_PORT:-8080}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# Check if running as root
check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (use sudo)"
        exit 1
    fi
}

# Detect OS
detect_os() {
    if [[ -f /etc/debian_version ]]; then
        OS="debian"
    elif [[ -f /etc/redhat-release ]]; then
        OS="rhel"
    elif [[ "$(uname)" == "Darwin" ]]; then
        OS="macos"
    else
        log_error "Unsupported OS"
        exit 1
    fi
    log_info "Detected OS: $OS"
}

# Install Caddy
install_caddy() {
    log_info "Installing Caddy..."

    case $OS in
        debian)
            apt-get update
            apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl
            curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
            curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
            apt-get update
            apt-get install -y caddy
            ;;
        rhel)
            yum install -y yum-plugin-copr
            yum copr enable -y @caddy/caddy
            yum install -y caddy
            ;;
        macos)
            if ! command -v brew &> /dev/null; then
                log_error "Homebrew is required. Install from https://brew.sh"
                exit 1
            fi
            brew install caddy
            ;;
    esac

    log_info "Caddy installed successfully"
}

# Configure Caddy
configure_caddy() {
    log_info "Configuring Caddy for domain: $DOMAIN"

    # Create log directory
    mkdir -p $HOME/marshal/logs

    # Backup existing Caddyfile if present
    if [[ -f /etc/caddy/Caddyfile ]]; then
        cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.backup.$(date +%Y%m%d%H%M%S)
    fi

    # Write Caddyfile
    cat > /etc/caddy/Caddyfile <<EOF
# ruvector-rag-server HTTPS configuration
# Auto-generated on $(date)

$DOMAIN {
    reverse_proxy localhost:$RAG_PORT {
        health_uri /health
        health_interval 30s
        health_timeout 5s

        header_up Host {host}
        header_up X-Real-IP {remote_host}
        header_up X-Forwarded-For {remote_host}
        header_up X-Forwarded-Proto {scheme}
    }

    request_body {
        max_size 100MB
    }

    encode gzip zstd

    log {
        output file $HOME/marshal/logs/caddy.log {
            roll_size 10MB
            roll_keep 5
        }
        format json
    }

    header {
        X-Content-Type-Options nosniff
        X-Frame-Options DENY
        Referrer-Policy strict-origin-when-cross-origin
        -Server
    }
}
EOF

    log_info "Caddyfile written to /etc/caddy/Caddyfile"
}

# Configure firewall
configure_firewall() {
    log_info "Configuring firewall..."

    case $OS in
        debian)
            if command -v ufw &> /dev/null; then
                ufw allow 80/tcp
                ufw allow 443/tcp
                log_info "UFW rules added for ports 80 and 443"
            fi
            ;;
        rhel)
            if command -v firewall-cmd &> /dev/null; then
                firewall-cmd --permanent --add-service=http
                firewall-cmd --permanent --add-service=https
                firewall-cmd --reload
                log_info "Firewalld rules added for HTTP and HTTPS"
            fi
            ;;
    esac
}

# Start Caddy service
start_caddy() {
    log_info "Starting Caddy service..."

    case $OS in
        debian|rhel)
            systemctl enable caddy
            systemctl restart caddy
            systemctl status caddy --no-pager
            ;;
        macos)
            brew services restart caddy
            ;;
    esac

    log_info "Caddy is running"
}

# Validate configuration
validate_config() {
    log_info "Validating Caddy configuration..."
    caddy validate --config /etc/caddy/Caddyfile
    log_info "Configuration is valid"
}

# Print summary
print_summary() {
    echo ""
    echo "=========================================="
    echo -e "${GREEN}HTTPS Setup Complete!${NC}"
    echo "=========================================="
    echo ""
    echo "Domain: https://$DOMAIN"
    echo "Backend: http://localhost:$RAG_PORT"
    echo ""
    echo "API Endpoints:"
    echo "  - Health:    https://$DOMAIN/health"
    echo "  - API Info:  https://$DOMAIN/api/info"
    echo "  - Query:     https://$DOMAIN/api/query"
    echo "  - Ingest:    https://$DOMAIN/api/ingest"
    echo "  - Documents: https://$DOMAIN/api/documents"
    echo ""
    echo "Logs: ~/marshal/logs/caddy.log"
    echo ""
    echo "Commands:"
    echo "  - Status:  sudo systemctl status caddy"
    echo "  - Logs:    sudo journalctl -u caddy -f"
    echo "  - Reload:  sudo systemctl reload caddy"
    echo ""
    log_warn "Make sure your DNS A record points to this server's IP!"
    echo ""
}

# Main
main() {
    if [[ -z "$DOMAIN" ]]; then
        echo "Usage: $0 <domain>"
        echo "Example: $0 rag.example.com"
        echo ""
        echo "Environment variables:"
        echo "  RAG_PORT - Backend port (default: 8080)"
        exit 1
    fi

    check_root
    detect_os

    if ! command -v caddy &> /dev/null; then
        install_caddy
    else
        log_info "Caddy already installed: $(caddy version)"
    fi

    configure_caddy
    validate_config
    configure_firewall
    start_caddy
    print_summary
}

main "$@"
