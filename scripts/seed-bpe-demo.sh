#!/bin/bash
# Marshal — BPE Demo Data Seeder
#
# Seeds the BPE (Business Process Engine) with example entity types,
# workflow definitions, and approval rules.
#
# Usage:
#   export BPE_API_KEY="your-jwt-token"
#   export SEED_ADMIN_EMAIL="admin@your-domain.com"
#   export SEED_ADMIN_PASSWORD="your-password"
#   ./scripts/seed-bpe-demo.sh
#
# Prerequisites:
#   - BPE server running
#   - Valid admin credentials
#   - curl and jq installed

set -euo pipefail

BPE_BASE_URL="${BPE_BASE_URL:-http://localhost:8090/bpe/api}"
APIKEY="${BPE_API_KEY:?Set BPE_API_KEY env var}"
SEED_ORG="${SEED_ORG:-demo-org}"
ADMIN_EMAIL="${SEED_ADMIN_EMAIL:?Set SEED_ADMIN_EMAIL env var}"
ADMIN_PASSWORD="${SEED_ADMIN_PASSWORD:?Set SEED_ADMIN_PASSWORD env var}"

echo "=== Marshal BPE Demo Seeder ==="
echo "Base URL: $BPE_BASE_URL"
echo "Organization: $SEED_ORG"
echo ""

# Get JWT token
echo "Authenticating..."
TOKEN=$(curl -s -X POST "${BPE_BASE_URL%/bpe/api}/api/auth/login" \
  -H "Content-Type: application/json" \
  -H "apikey: $APIKEY" \
  -d "{\"email\":\"$ADMIN_EMAIL\",\"password\":\"$ADMIN_PASSWORD\"}" | python3 -c "import sys,json; print(json.load(sys.stdin).get('token',''))")

if [ -z "$TOKEN" ]; then
  echo "ERROR: Authentication failed"
  exit 1
fi

AUTH="Authorization: Bearer $TOKEN"
CT="Content-Type: application/json"
AK="apikey: $APIKEY"

echo "Authenticated successfully."
echo ""

# Create entity types
echo "Creating entity types..."
for TYPE in "Customer" "Vendor" "Employee" "Project"; do
  curl -s -X POST "$BPE_BASE_URL/entity-types" \
    -H "$CT" -H "$AUTH" -H "$AK" \
    -d "{\"name\":\"$TYPE\",\"description\":\"$TYPE entity type\",\"organization_id\":\"$SEED_ORG\",\"schema\":{\"type\":\"object\"}}" > /dev/null
  echo "  Created: $TYPE"
done

# Create workflow definitions
echo "Creating workflow definitions..."
curl -s -X POST "$BPE_BASE_URL/workflows/definitions" \
  -H "$CT" -H "$AUTH" -H "$AK" \
  -d "{\"name\":\"Employee Onboarding\",\"description\":\"Standard onboarding workflow\",\"organization_id\":\"$SEED_ORG\",\"steps\":[{\"name\":\"HR Review\",\"type\":\"approval\"},{\"name\":\"IT Setup\",\"type\":\"task\"},{\"name\":\"Training\",\"type\":\"task\"}]}" > /dev/null
echo "  Created: Employee Onboarding"

curl -s -X POST "$BPE_BASE_URL/workflows/definitions" \
  -H "$CT" -H "$AUTH" -H "$AK" \
  -d "{\"name\":\"Purchase Approval\",\"description\":\"Purchase order approval workflow\",\"organization_id\":\"$SEED_ORG\",\"steps\":[{\"name\":\"Manager Review\",\"type\":\"approval\"},{\"name\":\"Finance Review\",\"type\":\"approval\"},{\"name\":\"PO Creation\",\"type\":\"task\"}]}" > /dev/null
echo "  Created: Purchase Approval"

echo ""
echo "=== Seed complete ==="
echo "Visit your Marshal dashboard to see the demo data."
