#!/bin/bash
# =============================================================================
# Baram Alpine VM Verification Script
# =============================================================================
# Runs the full post-migration verification checklist.
# Run as the mare user on the Alpine VM.
#
# Usage: bash scripts/alpine-migration/04-verify.sh
# =============================================================================
set -uo pipefail

BARAM_DIR="${HOME}/Baram"
PASS=0
FAIL=0
SKIP=0

check() {
    local desc="$1"
    shift
    if "$@" &>/dev/null; then
        echo "  [PASS] $desc"
        ((PASS++))
    else
        echo "  [FAIL] $desc"
        ((FAIL++))
    fi
}

check_output() {
    local desc="$1"
    shift
    local result
    result=$("$@" 2>&1) || true
    if [ -n "$result" ]; then
        echo "  [PASS] $desc: $result"
        ((PASS++))
    else
        echo "  [FAIL] $desc"
        ((FAIL++))
    fi
}

skip() {
    local desc="$1"
    echo "  [SKIP] $desc"
    ((SKIP++))
}

echo "============================================="
echo " Baram Alpine VM Verification"
echo " $(date '+%Y-%m-%d %H:%M:%S')"
echo "============================================="
echo ""

# --- 1. System ---
echo "[System]"
check_output "Alpine version" cat /etc/alpine-release
check "Bash available" command -v bash
check "SSH running" service sshd status
check "Cron running" service dcron status
echo ""

# --- 2. Docker ---
echo "[Docker]"
check "Docker installed" command -v docker
check "Docker running" docker info

CONTAINERS=("baram-postgres" "baram-opensearch" "baram-redis")
for c in "${CONTAINERS[@]}"; do
    if docker ps --format '{{.Names}}' | grep -q "^${c}$"; then
        echo "  [PASS] Container running: ${c}"
        ((PASS++))
    else
        echo "  [FAIL] Container not running: ${c}"
        ((FAIL++))
    fi
done
echo ""

# --- 3. Docker healthchecks ---
echo "[Docker Health]"
for c in "${CONTAINERS[@]}"; do
    HEALTH=$(docker inspect --format='{{.State.Health.Status}}' "$c" 2>/dev/null || echo "unknown")
    if [ "$HEALTH" = "healthy" ]; then
        echo "  [PASS] ${c}: ${HEALTH}"
        ((PASS++))
    else
        echo "  [FAIL] ${c}: ${HEALTH}"
        ((FAIL++))
    fi
done
echo ""

# --- 4. Rust toolchain ---
echo "[Rust]"
check_output "Rust compiler" rustc --version
check_output "Cargo" cargo --version
echo ""

# --- 5. Build ---
echo "[Build]"
BINARY="${BARAM_DIR}/target/release/baram"
if [ -x "$BINARY" ]; then
    echo "  [PASS] Release binary exists: ${BINARY}"
    ((PASS++))
    check_output "Binary version" "$BINARY" --version
else
    echo "  [FAIL] Release binary not found: ${BINARY}"
    ((FAIL++))
fi
echo ""

# --- 6. Data ---
echo "[Data]"
RAW_DIR="${BARAM_DIR}/output/raw"
if [ -d "$RAW_DIR" ]; then
    FILE_COUNT=$(find "$RAW_DIR" -name '*.md' | wc -l)
    echo "  [PASS] Crawled files: ${FILE_COUNT}"
    ((PASS++))
else
    echo "  [FAIL] Raw output directory not found"
    ((FAIL++))
fi

if [ -f "${BARAM_DIR}/output/crawl.db" ]; then
    DB_SIZE=$(du -h "${BARAM_DIR}/output/crawl.db" | cut -f1)
    echo "  [PASS] SQLite DB: ${DB_SIZE}"
    ((PASS++))
else
    echo "  [FAIL] SQLite DB not found"
    ((FAIL++))
fi
echo ""

# --- 7. OpenSearch ---
echo "[OpenSearch]"
OS_URL="${OPENSEARCH_URL:-http://localhost:9200}"

CLUSTER_HEALTH=$(curl -s "${OS_URL}/_cluster/health" 2>/dev/null)
if [ -n "$CLUSTER_HEALTH" ]; then
    STATUS=$(echo "$CLUSTER_HEALTH" | jq -r '.status' 2>/dev/null)
    echo "  [PASS] Cluster health: ${STATUS}"
    ((PASS++))

    DOC_COUNT=$(curl -s "${OS_URL}/baram-articles/_count" 2>/dev/null | jq -r '.count' 2>/dev/null || echo "0")
    if [ "$DOC_COUNT" != "0" ] && [ "$DOC_COUNT" != "null" ]; then
        echo "  [PASS] Index document count: ${DOC_COUNT}"
        ((PASS++))
    else
        echo "  [FAIL] Index empty or not found (count: ${DOC_COUNT})"
        ((FAIL++))
    fi
else
    echo "  [FAIL] Cannot connect to OpenSearch at ${OS_URL}"
    ((FAIL++))
    skip "Index document count"
fi
echo ""

# --- 8. PostgreSQL ---
echo "[PostgreSQL]"
PG_CHECK=$(docker exec baram-postgres pg_isready -U baram -d baram 2>/dev/null)
if [ $? -eq 0 ]; then
    echo "  [PASS] PostgreSQL ready"
    ((PASS++))
else
    echo "  [FAIL] PostgreSQL not ready"
    ((FAIL++))
fi
echo ""

# --- 9. Functional tests ---
echo "[Functional Tests]"

# Search test
if [ -x "$BINARY" ]; then
    SEARCH_RESULT=$("$BINARY" search "테스트" --k 3 2>&1 || true)
    if echo "$SEARCH_RESULT" | grep -qi "result\|score\|title\|article"; then
        echo "  [PASS] baram search works"
        ((PASS++))
    else
        echo "  [FAIL] baram search returned unexpected output"
        ((FAIL++))
    fi
else
    skip "baram search (no binary)"
fi

# API server check (try to start briefly)
if [ -x "$BINARY" ]; then
    "$BINARY" serve --port 18080 &
    API_PID=$!
    sleep 2

    API_HEALTH=$(curl -s http://localhost:18080/api/health 2>/dev/null || true)
    if echo "$API_HEALTH" | grep -qi "ok\|healthy\|status"; then
        echo "  [PASS] baram serve (API health check)"
        ((PASS++))
    else
        echo "  [FAIL] baram serve (API not responding)"
        ((FAIL++))
    fi

    kill "$API_PID" 2>/dev/null || true
    wait "$API_PID" 2>/dev/null || true
else
    skip "baram serve (no binary)"
fi
echo ""

# --- 10. Cron + Services ---
echo "[Cron & Services]"
if crontab -l 2>/dev/null | grep -q "crawl-only"; then
    echo "  [PASS] Crawl cron job configured"
    ((PASS++))
else
    echo "  [FAIL] Crawl cron job not found"
    ((FAIL++))
fi

if crontab -l 2>/dev/null | grep -q "index-only"; then
    echo "  [PASS] Index cron job configured"
    ((PASS++))
else
    echo "  [FAIL] Index cron job not found"
    ((FAIL++))
fi
echo ""

# --- 11. Claude Code ---
echo "[Claude Code]"
check "Claude Code installed" command -v claude
check "Node.js available" command -v node
check_output "Node.js version" node --version

if [ -d "${BARAM_DIR}/.claude" ]; then
    echo "  [PASS] .claude directory exists"
    ((PASS++))
else
    echo "  [FAIL] .claude directory not found"
    ((FAIL++))
fi
echo ""

# --- Summary ---
TOTAL=$((PASS + FAIL + SKIP))
echo "============================================="
echo " Verification Summary"
echo "============================================="
echo "  PASS: ${PASS}/${TOTAL}"
echo "  FAIL: ${FAIL}/${TOTAL}"
echo "  SKIP: ${SKIP}/${TOTAL}"
echo ""

if [ "$FAIL" -eq 0 ]; then
    echo "  All checks passed! Migration complete."
else
    echo "  ${FAIL} check(s) failed. Review above output."
fi
echo "============================================="

exit "$FAIL"
