# Baram - Claude Code Project Guide

## Project Overview
Rust 기반 네이버 뉴스 크롤러 + OpenSearch 벡터 검색 + LLM 온톨로지 시스템.
40,000+ lines of Rust code (v0.1.6, edition 2021, MSRV 1.80).

## Build & Test
```bash
cargo build --release          # Release build (~4min)
cargo test                     # All tests
cargo test -- extract_doc_id   # Specific test filter
cargo clippy                   # Lint
```

## CLI Commands
```bash
baram crawl --category politics --max-articles 100  # Crawl articles
baram index --input ./output/raw --batch-size 50    # Index to OpenSearch
baram index --input ./output/raw --since 2026-02-11 # Incremental index
baram index --input ./output/raw --force            # Full reindex
baram search "query" --k 10                         # Vector search
baram ontology --input ./output/raw --llm           # Ontology extraction
baram embedding-server --port 8090                  # Start embedding server
baram serve --port 8080                             # Start API server
```

## Architecture
```
crawl (Naver API → Markdown files)
  → index (Markdown → OpenSearch with embeddings)
  → ontology (LLM-based knowledge graph extraction)
  → serve (REST API + vector search)
```

### Key Modules
| Module | Path | Description |
|--------|------|-------------|
| Commands | `src/commands/` | CLI handlers (crawl, index, ontology, search, serve) |
| Crawler | `src/crawler/` | HTTP fetcher, pipeline, comment extractor, distributed |
| Embedding | `src/embedding/` | Vector generation, tokenizer, OpenSearch bulk indexing |
| Ontology | `src/ontology/` | LLM triple extraction, entity linking |
| Storage | `src/storage/` | SQLite, markdown writer, bloom filter dedup, checkpoint |
| Config | `src/config/` | AppConfig with TOML support |

### Data Flow
- **Crawl output**: `./output/raw/{oid}_{aid}_{title}.md` (YAML frontmatter + markdown)
- **Database**: `./output/crawl.db` (SQLite - crawl metadata)
- **Checkpoints**: `./checkpoints/` (JSON - resumable state)
- **OpenSearch**: `baram-articles` index (384-dim kNN vectors, nori analyzer)

## Infrastructure
### Systemd Services (user-level)
- `baram-crawl.timer` — 30분마다 크롤링 (flock: `.crawl.lock`)
- `baram-index.timer` — 2시간마다 인덱싱 (flock: `.index.lock`)
- `baram-embedding.service` — 임베딩 서버 상시 실행

### Docker Services
- PostgreSQL 18 (pgvector), OpenSearch 3.4, Redis 7
- Prometheus + Grafana monitoring
- barami-news-api, barami-news-dashboard, barami-admin-dashboard
- nginx reverse proxy (port 80)
- vLLM (Qwen2.5, port 8002)

## Code Conventions
- Error handling: `thiserror` + `anyhow`, custom `CrawlerError`/`StorageError`
- Async: `tokio` runtime, `futures::stream::buffer_unordered` for parallelism
- Logging: `tracing` with structured fields
- Retry: exponential backoff via `utils::retry::with_retry`
- ID format: `{oid}_{aid}` (both numeric)
- Tests: `#[cfg(test)] mod tests` in-file, integration tests in `tests/`

## Environment Variables
```
OPENSEARCH_URL=http://localhost:9200
OPENSEARCH_INDEX=baram-articles
EMBEDDING_SERVER_URL=http://localhost:8090
RUST_LOG=info  # or baram::crawler=debug
```

## Important Patterns
- **Incremental indexing**: Checkpoint pre-filtering by filename ID extraction (no file I/O)
- **Batch embedding**: `/embed/batch` endpoint (up to 100 texts per call)
- **3-tier dedup**: Bloom filter → HashSet cache → DB query
- **Parallel parsing**: `tokio::task::spawn_blocking` + `buffer_unordered`
- **Atomic checkpoint saves**: temp file + rename
