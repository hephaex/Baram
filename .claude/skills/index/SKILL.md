---
name: index
description: Run Baram article indexing to OpenSearch. Use when user wants to index or re-index articles.
disable-model-invocation: true
argument-hint: [--since YYYY-MM-DD | --force]
allowed-tools: Bash
---

Run the Baram indexer with the given arguments.

Arguments: $ARGUMENTS

Behavior:
- If `--force` is specified: Full re-index (deletes existing index, re-indexes all files)
- If `--since DATE` is specified: Incremental index from that date
- If no arguments: Use `--since` with 2 days ago (default incremental)

Steps:
1. Check if another index process is already running: `ps aux | grep 'baram index' | grep -v grep`
2. If running, warn the user and ask whether to proceed
3. Check embedding server is up: `curl -sf localhost:8090/health || echo "Embedding server not running!"`
4. Run the index command in background with nohup for long operations:
   - Default: `./target/release/baram index --input ./output/raw --batch-size 100 --since $(date -d '2 days ago' +%Y-%m-%d)`
   - Force: `./target/release/baram index --input ./output/raw --batch-size 100 --force`
   - Custom: `./target/release/baram index --input ./output/raw --batch-size 100 $ARGUMENTS`
5. For `--force` or large index jobs, run in background with `nohup ... > /tmp/reindex.log 2>&1 &` and tell user to check with `tail -f /tmp/reindex.log`
6. Report initial output and expected duration in Korean
