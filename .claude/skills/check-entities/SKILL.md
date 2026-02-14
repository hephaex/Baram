---
name: check-entities
description: Check for remaining HTML entities in crawled markdown files. Use when user wants to verify HTML entity cleanup.
allowed-tools: Bash
---

Check if any HTML entities remain in crawled markdown files.

Steps:
1. Count files with entities: `grep -rlc '&#x[0-9a-fA-F]\+;\|&quot;\|&amp;\|&lt;\|&gt;\|&apos;\|&#[0-9]\+;' ./output/raw/ | wc -l`
2. If any found, show entity type breakdown: `grep -roh '&#x[0-9a-fA-F]\+;\|&quot;\|&amp;\|&lt;\|&gt;\|&apos;\|&#[0-9]\+;' ./output/raw/ | sort | uniq -c | sort -rn | head -20`
3. Show sample files: `grep -rl '&#x[0-9a-fA-F]\+;' ./output/raw/ | head -5`
4. Report results in Korean: clean or showing remaining issues
