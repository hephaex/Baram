#!/bin/bash
# =============================================================================
# Baram Data Migration Script
# =============================================================================
# Run this on the CURRENT server to transfer data to the Alpine VM.
#
# Usage:
#   ALPINE_IP=192.168.1.100 bash scripts/alpine-migration/02-migrate-data.sh
#
# Prerequisites:
#   - SSH key-based access to mare@ALPINE_IP
#   - Alpine VM bootstrap complete (01-bootstrap.sh)
#   - Project cloned on Alpine VM (~/Baram exists)
# =============================================================================
set -euo pipefail

ALPINE_IP="${ALPINE_IP:?Set ALPINE_IP environment variable (e.g. ALPINE_IP=192.168.1.100)}"
ALPINE_USER="${ALPINE_USER:-mare}"
REMOTE="${ALPINE_USER}@${ALPINE_IP}"

SOURCE_DIR="${SOURCE_DIR:-/data/Simon/Baram}"
REMOTE_DIR="${REMOTE_DIR:-~/Baram}"

echo "============================================="
echo " Baram Data Migration"
echo " Source: ${SOURCE_DIR}"
echo " Target: ${REMOTE}:${REMOTE_DIR}"
echo "============================================="
echo ""

# --- Pre-flight checks ---
echo "[0/6] Pre-flight checks..."
if ! ssh -q -o ConnectTimeout=5 "$REMOTE" exit; then
    echo "ERROR: Cannot connect to ${REMOTE}"
    echo "  - Check SSH key: ssh-copy-id ${REMOTE}"
    echo "  - Check Alpine VM is running"
    exit 1
fi

ssh "$REMOTE" "mkdir -p ${REMOTE_DIR}/{output,checkpoints,logs,models}"
echo "  -> Connection OK, directories ready"

# --- Step 1: Crawled data (122K+ markdown files) ---
echo ""
echo "[1/6] Syncing crawled data (this may take a while)..."
echo "  Source: ${SOURCE_DIR}/output/"
rsync -avz --progress \
    --exclude='*.lock' \
    --exclude='*.tmp' \
    "${SOURCE_DIR}/output/" \
    "${REMOTE}:${REMOTE_DIR}/output/"
echo "  -> Crawled data sync complete"

# --- Step 2: Checkpoints ---
echo ""
echo "[2/6] Syncing checkpoints..."
rsync -avz \
    "${SOURCE_DIR}/checkpoints/" \
    "${REMOTE}:${REMOTE_DIR}/checkpoints/"
echo "  -> Checkpoints sync complete"

# --- Step 3: Models cache (if exists) ---
echo ""
echo "[3/6] Syncing model cache..."
if [ -d "${SOURCE_DIR}/models" ] && [ "$(ls -A "${SOURCE_DIR}/models" 2>/dev/null)" ]; then
    rsync -avz --progress \
        "${SOURCE_DIR}/models/" \
        "${REMOTE}:${REMOTE_DIR}/models/"
    echo "  -> Models sync complete"
else
    echo "  -> No models directory, skipping"
fi

# --- Step 4: Logs (selective - recent only) ---
echo ""
echo "[4/6] Syncing recent logs..."
if [ -d "${SOURCE_DIR}/logs" ]; then
    rsync -avz \
        --include='*.log' \
        --exclude='*' \
        "${SOURCE_DIR}/logs/" \
        "${REMOTE}:${REMOTE_DIR}/logs/"
    echo "  -> Logs sync complete"
else
    echo "  -> No logs directory, skipping"
fi

# --- Step 5: .claude project config ---
echo ""
echo "[5/6] Syncing Claude Code config..."
rsync -avz \
    --exclude='*.jsonl' \
    "${SOURCE_DIR}/.claude/" \
    "${REMOTE}:${REMOTE_DIR}/.claude/"
echo "  -> Claude config sync complete"

# --- Step 6: PostgreSQL dump + restore ---
echo ""
echo "[6/6] PostgreSQL migration..."

PG_CONTAINER="baram-postgres"
PG_USER="${POSTGRES_USER:-baram}"
PG_DB="${POSTGRES_DB:-baram}"
DUMP_FILE="/tmp/baram_pg_dump.sql"

# Check if local postgres container is running
if docker ps --format '{{.Names}}' | grep -q "^${PG_CONTAINER}$"; then
    echo "  Dumping PostgreSQL..."
    docker exec "$PG_CONTAINER" pg_dump -U "$PG_USER" "$PG_DB" > "$DUMP_FILE"
    DUMP_SIZE=$(du -h "$DUMP_FILE" | cut -f1)
    echo "  -> Dump created: ${DUMP_FILE} (${DUMP_SIZE})"

    echo "  Transferring dump to Alpine VM..."
    scp "$DUMP_FILE" "${REMOTE}:/tmp/baram_pg_dump.sql"

    echo "  Restoring on Alpine VM..."
    echo "  NOTE: Docker services must be running on Alpine VM first."
    echo "  Run on Alpine VM:"
    echo "    cd ~/Baram/docker"
    echo "    docker compose -f docker-compose.yml -f docker-compose.alpine.yml up -d postgres"
    echo "    # Wait for postgres to be healthy"
    echo "    docker exec -i baram-postgres psql -U ${PG_USER} ${PG_DB} < /tmp/baram_pg_dump.sql"
    echo ""

    rm -f "$DUMP_FILE"
else
    echo "  -> PostgreSQL container not running, skipping dump"
    echo "  -> Run manually after Docker is set up on Alpine VM"
fi

# --- Summary ---
echo ""
echo "============================================="
echo " Data Migration Complete!"
echo "============================================="
echo ""
echo " File counts:"
ssh "$REMOTE" "echo '  Crawled files: ' && find ${REMOTE_DIR}/output/raw -name '*.md' 2>/dev/null | wc -l"
ssh "$REMOTE" "echo '  Checkpoints:   ' && ls ${REMOTE_DIR}/checkpoints/ 2>/dev/null | wc -l"
echo ""
echo " Next steps on Alpine VM:"
echo "   1. Start Docker services:"
echo "      cd ~/Baram/docker"
echo "      docker compose -f docker-compose.yml -f docker-compose.alpine.yml up -d"
echo "   2. Restore PostgreSQL (if dump was created):"
echo "      docker exec -i baram-postgres psql -U baram baram < /tmp/baram_pg_dump.sql"
echo "   3. Rebuild OpenSearch index:"
echo "      cd ~/Baram"
echo "      ./target/release/baram index --input ./output/raw --force --batch-size 50"
echo "   4. Set up cron + services:"
echo "      sudo bash scripts/alpine-migration/03-services.sh"
echo "============================================="
