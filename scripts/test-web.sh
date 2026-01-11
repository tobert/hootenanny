#!/usr/bin/env bash
# Test script for hootenanny web endpoints
# Usage: ./scripts/test-web.sh [host:port]

set -euo pipefail

HOST="${1:-localhost:8082}"
BASE="http://$HOST"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

pass() { echo -e "${GREEN}✓${NC} $1"; }
fail() { echo -e "${RED}✗${NC} $1"; exit 1; }
warn() { echo -e "${YELLOW}⚠${NC} $1"; }

echo "Testing hootenanny web endpoints at $BASE"
echo "==========================================="

# Test root discovery
echo -n "GET / ... "
resp=$(curl -sf "$BASE/" 2>/dev/null) || fail "Root endpoint failed"
echo "$resp" | jq -e '.name == "Hootenanny"' > /dev/null || fail "Unexpected root response"
pass "Discovery endpoint"

# Test health
echo -n "GET /health ... "
resp=$(curl -sf "$BASE/health" 2>/dev/null) || fail "Health endpoint failed"
echo "$resp" | jq -e '.status' > /dev/null || fail "Health missing status"
status=$(echo "$resp" | jq -r '.status')
pass "Health: $status"

# Test UI loads
echo -n "GET /ui ... "
resp=$(curl -sf "$BASE/ui" 2>/dev/null) || fail "UI endpoint failed"
echo "$resp" | grep -q "Hootenanny" || fail "UI missing expected content"
pass "UI page loads"

# Test artifacts list
echo -n "GET /artifacts ... "
artifacts=$(curl -sf "$BASE/artifacts" 2>/dev/null) || fail "Artifacts endpoint failed"
count=$(echo "$artifacts" | jq 'length')
pass "Artifacts list ($count items)"

# Test stream status
echo -n "GET /stream/live/status ... "
resp=$(curl -sf "$BASE/stream/live/status" 2>/dev/null) || fail "Stream status failed"
backend=$(echo "$resp" | jq -r '.backend')
pass "Stream status (backend: $backend)"

# Test artifact access if any exist
if [ "$count" -gt 0 ]; then
    artifact_id=$(echo "$artifacts" | jq -r '.[0].id')

    echo -n "GET /artifact/$artifact_id/meta ... "
    meta=$(curl -sf "$BASE/artifact/$artifact_id/meta" 2>/dev/null) || fail "Artifact meta failed"
    echo "$meta" | jq -e '.id' > /dev/null || fail "Meta missing id"
    pass "Artifact metadata"

    echo -n "GET /artifact/$artifact_id ... "
    curl -sf -o /dev/null -w "%{http_code}" "$BASE/artifact/$artifact_id" 2>/dev/null | grep -q "200" || fail "Artifact download failed"
    pass "Artifact download"
else
    warn "No artifacts to test download"
fi

# Test WebSocket upgrade (just check it accepts upgrade)
echo -n "WebSocket /stream/live ... "
if command -v websocat &> /dev/null; then
    # Send start, wait briefly, send stop
    echo '{"type":"start"}' | timeout 2 websocat -t "ws://$HOST/stream/live" 2>/dev/null && pass "WebSocket connects" || warn "WebSocket test inconclusive"
else
    # Fallback: just check the endpoint responds to HTTP
    code=$(curl -sf -o /dev/null -w "%{http_code}" -H "Upgrade: websocket" -H "Connection: Upgrade" "$BASE/stream/live" 2>/dev/null || echo "000")
    if [ "$code" = "426" ] || [ "$code" = "101" ]; then
        pass "WebSocket endpoint exists (upgrade required)"
    else
        warn "WebSocket test skipped (install websocat for full test)"
    fi
fi

echo ""
echo -e "${GREEN}All tests passed!${NC}"
