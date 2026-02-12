#!/bin/bash
# Continuous crawler + indexer script for Baram
cd /home/mare/Baram

while true; do
    echo "[$(date)] Starting crawl cycle..."
    ./target/release/baram crawl --verbose >> logs/crawl-$(date +%Y%m%d).log 2>&1

    echo "[$(date)] Starting indexing..."
    ./target/release/baram index --input ./output/raw --verbose >> logs/index-$(date +%Y%m%d).log 2>&1

    echo "[$(date)] Cycle complete. Sleeping for 30 minutes..."
    sleep 1800
done
