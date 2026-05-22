#!/bin/bash
# Setup HTTPS with API Key authentication for frontend-only access
# Usage: ./setup-authenticated-https.sh <domain> [api_key]

set -e

DOMAIN="${1:-}"
API_KEY="${2:-$(openssl rand -hex 32)}"
RAG_PORT="${RAG_PORT:-8080}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_secret() { echo -e "${BLUE}[SECRET]${NC} $1"; }

if [[ -z "$DOMAIN" ]]; then
    echo "Usage: $0 <domain> [api_key]"
    echo ""
    echo "Examples:"
    echo "  $0 rag.example.com                    # Auto-generate API key"
    echo "  $0 rag.example.com my-secret-key-123  # Use specific API key"
    echo ""
    echo "Environment variables:"
    echo "  RAG_PORT - Backend port (default: 8080)"
    exit 1
fi

if [[ $EUID -ne 0 ]]; then
    log_error "Run as root: sudo $0 $DOMAIN"
    exit 1
fi

# Install Caddy if not present
if ! command -v caddy &> /dev/null; then
    log_info "Installing Caddy..."
    apt-get update
    apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
    apt-get update
    apt-get install -y caddy
fi

# Create directories
mkdir -p $HOME/marshal/logs
mkdir -p /etc/caddy

# Backup existing config
if [[ -f /etc/caddy/Caddyfile ]]; then
    cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.backup.$(date +%Y%m%d%H%M%S)
fi

log_info "Configuring Caddy with API Key authentication..."

# Write Caddyfile with embedded API key
cat > /etc/caddy/Caddyfile <<EOF
# ruvector-rag-server with API Key Authentication
# Generated: $(date)
# Domain: $DOMAIN

$DOMAIN {
    # API Key validation
    @unauthorized {
        not header X-API-Key "$API_KEY"
    }

    # Health checks - no auth required (for monitoring)
    @health_check {
        path /health /ready
    }

    # Allow health checks without auth
    handle @health_check {
        reverse_proxy localhost:$RAG_PORT
    }

    # Block unauthorized requests
    handle @unauthorized {
        respond "Unauthorized - Valid X-API-Key header required" 401
    }

    # Authenticated requests
    handle {
        reverse_proxy localhost:$RAG_PORT {
            health_uri /health
            health_interval 30s

            header_up Host {host}
            header_up X-Real-IP {remote_host}
            header_up X-Forwarded-For {remote_host}
            header_up X-Forwarded-Proto {scheme}

            # Strip API key before forwarding to backend
            header_up -X-API-Key
        }
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

# Validate and restart
log_info "Validating configuration..."
caddy validate --config /etc/caddy/Caddyfile

log_info "Restarting Caddy..."
systemctl enable caddy
systemctl restart caddy

# Configure firewall
if command -v ufw &> /dev/null; then
    ufw allow 80/tcp
    ufw allow 443/tcp
fi

# Save API key securely
API_KEY_FILE="/etc/caddy/.api_key"
echo "$API_KEY" > "$API_KEY_FILE"
chmod 600 "$API_KEY_FILE"
chown root:root "$API_KEY_FILE"

echo ""
echo "=========================================="
echo -e "${GREEN}Authenticated HTTPS Setup Complete!${NC}"
echo "=========================================="
echo ""
echo "Domain: https://$DOMAIN"
echo ""
log_secret "API Key: $API_KEY"
echo ""
echo "API Key saved to: $API_KEY_FILE"
echo ""
echo "=========================================="
echo "FRONTEND CONFIGURATION"
echo "=========================================="
echo ""
echo "Add this header to all API requests:"
echo ""
echo "  X-API-Key: $API_KEY"
echo ""
echo "Example fetch (JavaScript):"
echo ""
cat <<'JSEOF'
const API_KEY = process.env.REACT_APP_RAG_API_KEY;
const API_URL = 'https://DOMAIN';

async function queryRAG(question) {
  const response = await fetch(`${API_URL}/api/query`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-API-Key': API_KEY,
    },
    body: JSON.stringify({ question }),
  });
  return response.json();
}
JSEOF
echo ""
echo "(Replace DOMAIN with: $DOMAIN)"
echo ""
echo "=========================================="
echo "TEST COMMANDS"
echo "=========================================="
echo ""
echo "# Without API key (should fail with 401):"
echo "curl https://$DOMAIN/api/info"
echo ""
echo "# With API key (should succeed):"
echo "curl -H 'X-API-Key: $API_KEY' https://$DOMAIN/api/info"
echo ""
echo "# Health check (no auth required):"
echo "curl https://$DOMAIN/health"
echo ""
