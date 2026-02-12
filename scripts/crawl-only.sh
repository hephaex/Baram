#!/bin/bash
# Baram Crawl-Only Script (with flock)
set -euo pipefail

export HOME="${HOME:-/home/mare}"
export PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin:$PATH"

PROJECT_DIR="/home/mare/Baram"
LOCK_FILE="$PROJECT_DIR/.crawl.lock"
LOG_FILE="$PROJECT_DIR/logs/crawl-$(date +%Y%m%d).log"

mkdir -p "$PROJECT_DIR/logs"

# Atomic lock - prevents duplicate runs
exec 200>"$LOCK_FILE"
if ! flock -n 200; then
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] Crawl already running. Skipping." >> "$LOG_FILE"
    exit 0
fi
echo $$ >&200

cd "$PROJECT_DIR"

CATEGORIES=("politics" "economy" "society" "culture" "world" "it")
MAX_ARTICLES=300

echo "[$(date '+%Y-%m-%d %H:%M:%S')] Starting crawl cycle..." >> "$LOG_FILE"

for category in "${CATEGORIES[@]}"; do
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] Crawling: $category" >> "$LOG_FILE"
    ./target/release/baram crawl \
        --category "$category" \
        --max-articles "$MAX_ARTICLES" \
        --output ./output/raw \
        --skip-existing >> "$LOG_FILE" 2>&1 || true
done

echo "[$(date '+%Y-%m-%d %H:%M:%S')] Crawl cycle complete" >> "$LOG_FILE"
