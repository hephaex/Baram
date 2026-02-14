---
name: search
description: Search indexed articles using vector search. Use when user wants to find articles by query.
argument-hint: [search query]
allowed-tools: Bash
---

Search for articles matching the user's query using Baram's vector search.

Query: $ARGUMENTS

Run: `./target/release/baram search "$ARGUMENTS" --k 10`

Present the results in a clean, readable format in Korean with:
- Article title
- Publisher and date
- Relevance score
- Brief content snippet
