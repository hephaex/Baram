#!/bin/bash
# Baram Hourly Crawl Script
# Runs every hour to collect new articles from all categories

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
LOG_DIR="$PROJECT_DIR/logs"
OUTPUT_DIR="$PROJECT_DIR/output/raw"

# Create log directory
mkdir -p "$LOG_DIR"

# Log file with date
LOG_FILE="$LOG_DIR/crawl-$(date +%Y%m%d).log"

# Categories to crawl
CATEGORIES=("politics" "economy" "society" "culture" "world" "it")

# Articles per category per hour
MAX_ARTICLES=50

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

log "========================================="
log "Starting hourly crawl"
log "========================================="

cd "$PROJECT_DIR"

# Crawl each category
for category in "${CATEGORIES[@]}"; do
    log "Crawling category: $category"

    if cargo run --release -- crawl \
        --category "$category" \
        --max-articles "$MAX_ARTICLES" \
        --output "$OUTPUT_DIR" \
        --skip-existing \
        2>&1 | tee -a "$LOG_FILE"; then
        log "✓ $category completed"
    else
        log "✗ $category failed"
    fi
done

# Index new articles to OpenSearch
log "Indexing to OpenSearch..."
for f in "$OUTPUT_DIR"/*.md; do
    if [ -f "$f" ]; then
        id=$(grep "^id:" "$f" 2>/dev/null | sed 's/id: //' || echo "")
        if [ -n "$id" ]; then
            oid=$(grep "^oid:" "$f" | sed 's/oid: //')
            aid=$(grep "^aid:" "$f" | sed 's/aid: //')
            title=$(grep "^title:" "$f" | sed 's/title: "//' | sed 's/"$//')
            publisher=$(grep "^publisher:" "$f" | sed 's/publisher: //')

            curl -s -X POST "http://localhost:9200/baram-articles/_doc/$id" \
                -H "Content-Type: application/json" \
                -d "{
                    \"id\": \"$id\",
                    \"oid\": \"$oid\",
                    \"aid\": \"$aid\",
                    \"title\": \"$title\",
                    \"publisher\": \"$publisher\",
                    \"crawled_at\": \"$(date -Iseconds)\"
                }" > /dev/null 2>&1
        fi
    fi
done

log "Indexing completed"

# Show stats
TOTAL_ARTICLES=$(find "$OUTPUT_DIR" -name "*.md" | wc -l)
INDEX_COUNT=$(curl -s "http://localhost:9200/baram-articles/_count" | grep -o '"count":[0-9]*' | cut -d: -f2)

log "========================================="
log "Crawl Summary"
log "  Total articles: $TOTAL_ARTICLES"
log "  Indexed: $INDEX_COUNT"
log "========================================="
