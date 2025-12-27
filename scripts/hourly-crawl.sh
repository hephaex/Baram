#!/bin/bash
# Baram Automated Crawl & Processing Script
# Runs periodically to collect, index, and process articles

set -euo pipefail

# Set environment for cron
export HOME="${HOME:-/home/mare}"
export PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
LOG_DIR="$PROJECT_DIR/logs"
OUTPUT_DIR="$PROJECT_DIR/output/raw"
ONTOLOGY_DIR="$PROJECT_DIR/output/ontology"
DB_PATH="$PROJECT_DIR/output/crawl.db"

# Create directories
mkdir -p "$LOG_DIR" "$OUTPUT_DIR" "$ONTOLOGY_DIR"

# Log file with date
LOG_FILE="$LOG_DIR/crawl-$(date +%Y%m%d).log"

# Categories to crawl
CATEGORIES=("politics" "economy" "society" "culture" "world" "it")

# Articles per category
MAX_ARTICLES=50

# Batch size for indexing
BATCH_SIZE=50

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

log "========================================="
log "Starting automated crawl & processing"
log "========================================="

cd "$PROJECT_DIR"

# =========================================
# Phase 1: Crawl articles
# =========================================
log "Phase 1: Crawling articles..."

CRAWL_SUCCESS=0
CRAWL_FAIL=0

for category in "${CATEGORIES[@]}"; do
    log "  Crawling: $category"

    if cargo run --release -- crawl \
        --category "$category" \
        --max-articles "$MAX_ARTICLES" \
        --output "$OUTPUT_DIR" \
        --skip-existing \
        2>&1 | tee -a "$LOG_FILE"; then
        log "  ✓ $category completed"
        ((CRAWL_SUCCESS++))
    else
        log "  ✗ $category failed"
        ((CRAWL_FAIL++))
    fi
done

log "Phase 1 complete: $CRAWL_SUCCESS success, $CRAWL_FAIL failed"

# =========================================
# Phase 2: Index to OpenSearch with embeddings
# =========================================
log "Phase 2: Indexing with embeddings..."

if cargo run --release -- index \
    --input "$OUTPUT_DIR" \
    --batch-size "$BATCH_SIZE" \
    2>&1 | tee -a "$LOG_FILE"; then
    log "  ✓ Indexing completed"
else
    log "  ✗ Indexing failed"
fi

# =========================================
# Phase 3: Extract ontology
# =========================================
log "Phase 3: Extracting ontology..."

ONTOLOGY_OUTPUT="$ONTOLOGY_DIR/ontology-$(date +%Y%m%d-%H%M).json"

if cargo run --release -- ontology \
    --input "$OUTPUT_DIR" \
    --format json \
    --output "$ONTOLOGY_OUTPUT" \
    2>&1 | tee -a "$LOG_FILE"; then
    log "  ✓ Ontology extracted: $ONTOLOGY_OUTPUT"
else
    log "  ✗ Ontology extraction failed"
fi

# =========================================
# Phase 4: Statistics
# =========================================
log "Phase 4: Collecting statistics..."

TOTAL_ARTICLES=$(find "$OUTPUT_DIR" -name "*.md" 2>/dev/null | wc -l)
DB_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM crawled_urls WHERE status = 'success';" 2>/dev/null || echo "0")
INDEX_COUNT=$(curl -s "http://localhost:9200/baram-articles/_count" 2>/dev/null | grep -o '"count":[0-9]*' | cut -d: -f2 || echo "0")
ONTOLOGY_FILES=$(find "$ONTOLOGY_DIR" -name "*.json" 2>/dev/null | wc -l)

log "========================================="
log "Processing Summary"
log "  Categories crawled: $CRAWL_SUCCESS/${#CATEGORIES[@]}"
log "  Total articles (files): $TOTAL_ARTICLES"
log "  Database records: $DB_COUNT"
log "  OpenSearch indexed: $INDEX_COUNT"
log "  Ontology files: $ONTOLOGY_FILES"
log "========================================="
log "Automated processing completed"
