#!/bin/bash
# Baram Index-Only Script (with flock)
set -euo pipefail

export HOME="${HOME:-/home/mare}"
export PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin:$PATH"

PROJECT_DIR="/home/mare/Baram"
LOCK_FILE="$PROJECT_DIR/.index.lock"
LOG_FILE="$PROJECT_DIR/logs/index-$(date +%Y%m%d).log"

mkdir -p "$PROJECT_DIR/logs"

# Atomic lock - prevents duplicate runs
exec 200>"$LOCK_FILE"
if ! flock -n 200; then
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] Index already running. Skipping." >> "$LOG_FILE"
    exit 0
fi
echo $$ >&200

cd "$PROJECT_DIR"

echo "[$(date '+%Y-%m-%d %H:%M:%S')] Starting index..." >> "$LOG_FILE"

./target/release/baram index \
    --input ./output/raw \
    --batch-size 50 >> "$LOG_FILE" 2>&1

echo "[$(date '+%Y-%m-%d %H:%M:%S')] Index complete" >> "$LOG_FILE"
