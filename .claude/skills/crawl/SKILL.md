---
name: crawl
description: Run Baram news crawler with specified options. Use when user wants to crawl articles.
disable-model-invocation: true
argument-hint: [--category politics|economy|society|life|world|it --max-articles 100]
allowed-tools: Bash
---

Run the Baram crawler with the given arguments. Default behavior if no arguments: crawl all 6 categories with 100 max articles.

Arguments: $ARGUMENTS

Steps:
1. Parse arguments. Defaults: `--max-articles 100`, categories: politics,economy,society,life,world,it
2. Run: `./target/release/baram crawl $ARGUMENTS`
3. If no specific arguments given, run the standard crawl script: `bash scripts/crawl-only.sh`
4. Report the crawl summary (total, successful, failed) in Korean
