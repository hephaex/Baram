# Baram - Claude Code Project Guide

## !! 에이전트 필수 규칙 — 세션 시작 시 반드시 읽을 것 !!

**모든 에이전트는 작업 시작 전 아래 파일을 반드시 읽어야 합니다:**

1. **`.claude/execution-log.md`** — 이전 파이프라인 실행 이력 (어디서 중단? 에러? 교훈?)
2. **`PLAN.md`** — 다음에 해야 할 작업, Phase별 체크리스트, 즉시 해야 할 일
3. **`PROGRESS.md`** — 완료된 작업, 진행 중인 작업, 알려진 이슈
4. **`.claude/references/INDEX.md`** — 기술 조사 자료 인덱스

### 실행 이력 시스템
- **`.claude/execution-log.md`**: PM이 매 파이프라인 Step마다 기록하는 상세 이력
  - 어떤 sub-agent가 실행되었고, 결과(SUCCESS/FAIL/FIX/BLOCK)는 무엇인지
  - 에러 내용과 수정 시도 내역
  - **교훈(Lessons)**: 작업에서 배운 것 — 다음 작업 시 sub-agent에게 전달
- PM은 sub-agent 호출 시 관련 이력을 `## 이전 실행 이력` 섹션으로 전달
- 같은 실수 반복 방지가 핵심 목적

### 작업 시 규칙
- **시작**: execution-log.md에서 중단된 작업 확인 → 있으면 이어서, 없으면 PLAN.md 첫 `[ ]`
- **진행 중**: execution-log.md에 Step별 결과 즉시 기록
- **완료 시**: execution-log.md에 파이프라인 테이블 + 교훈 기록, PROGRESS.md + PLAN.md 업데이트
- **중단 시**: execution-log.md에 중단 지점 + 에러 상세 기록
- **기술 조사 시**: `/ref [키워드]`로 기존 조사 자료 확인 → 없으면 `/add-ref [주제]`로 추가

### 자율 운영 모드 (Autonomous CI/CD)

**PM 에이전트는 이슈 발생 전까지 자동으로 계속 진행한다.**

자동 허가 사항:
- **자동 커밋**: 검증(clippy + test) 통과 후 변경 파일 커밋 (feat:/fix:/refactor: 접두사)
- **자동 진행**: 한 작업 완료 즉시 다음 PLAN.md 미완료 항목으로 이동
- **자동 수정**: 리뷰 FIX, clippy 경고, 테스트 실패 → sub-agent로 자동 수정 (최대 2회)
- **자동 조사**: Phase 첫 작업 시 references 자동 업데이트

사용자 확인이 필요한 STOP 조건:
- Cargo.toml 의존성 추가/변경
- Docker/인프라/systemd 변경
- 외부 서비스 신규 설정 (Neo4j 등)
- 보안 블로커 (BLOCK 판정)
- 동일 에러 3회 반복 (ESCALATE)
- 설계 판단이 필요한 선택지
- Phase 전체 완료 (release build 후 다음 Phase 진행 여부)

### Context Guard — /compact로 컨텍스트 유지

장기 자동 실행 시 컨텍스트 윈도우가 차면 이전 이력을 잃는다.
PM은 **2개 작업 완료마다** `checkpoint_and_compact()`를 실행한다.

```
checkpoint_and_compact() 절차:
1. execution-log.md에 현재까지 진행 상태 기록
2. PLAN.md, PROGRESS.md 최신 상태 확인 (아직 안 쓴 업데이트 flush)
3. /compact 실행
4. /compact 후 execution-log.md, PLAN.md 다시 읽기 → 상태 복원
5. 루프 재개
```

추가 /compact 타이밍:
- 에러 복구가 2회 이상 반복된 직후 (에러 로그가 컨텍스트를 많이 소비)
- sub-agent가 매우 긴 출력을 반환한 직후

### Sub-Agent 통신 프로토콜

모든 sub-agent는 작업 완료 시 `---RESULT---` ... `---END---` 블록을 반환한다.
PM은 이 블록을 파싱하여 자동으로 다음 단계를 결정한다.

```
researcher  → STATUS: SUCCESS|FAIL, FILE: [...], SUMMARY: ...
implementer → STATUS: SUCCESS|FAIL, FILES: [...], TESTS: [...], SUMMARY: ...
reviewer    → VERDICT: PASS|FIX|BLOCK, FIXES: [...], REASON: ...
verifier    → STATUS: PASS|FAIL, CLIPPY: ..., TESTS: ..., BUILD: ..., ERROR: ...
```

### 에러 복구 — 3단계 자동 복구

```
에러 발생 (구현 FAIL / 검증 FAIL / 리뷰 FIX)
  ├─ 1차 (auto): PM이 에러 분석 → implementer에게 수정 지시
  ├─ 2차 (auto): debugger agent에게 이력 포함 위임
  └─ 3차: ESCALATE → STOP (사용자에게 보고)

리뷰 FIX 반복:
  FIX 1~2회 → implementer로 수정 → 재리뷰
  FIX 3회 → ESCALATE
```

### 에이전트 오케스트레이션 (.claude/agents/)

| 에이전트 | 역할 | 모델 | 호출 시점 |
|---------|------|------|----------|
| `pm` | 자율 PM — 자동 루프, /compact, 에러 복구 | opus | Phase 작업 요청 시 |
| `implementer` | Rust 구현 — 구조화된 결과(SUCCESS/FAIL) 반환 | opus | PM → Step B |
| `reviewer` | 코드 리뷰 — 구조화된 판정(PASS/FIX/BLOCK) 반환 | sonnet | PM → Step C |
| `verifier` | 빌드/테스트 검증 — 구조화된 결과(PASS/FAIL) 반환 | sonnet | PM → Step D |
| `researcher` | 기술 조사 — references에 저장, 요약 반환 | sonnet | PM → Step A |

**PM 자동 루프** (이력 참조 + /compact):
```
세션 시작 → execution-log.md + PLAN.md + PROGRESS.md + INDEX.md 읽기

retry_count = 0
task_count = 0

WHILE 미완료 작업:
  IF task_count > 0 AND task_count % 2 == 0:
    checkpoint_and_compact()          ← 2개 작업마다 컨텍스트 정리

  Gate Check → 조사(A) → 구현(B) → 리뷰(C) → 검증(D) → 커밋(E) → 이력기록(F)

  SUCCESS → retry_count=0, task_count++, 다음 작업
  RETRY   → retry_count++ (3회 시 ESCALATE)
  ESCALATE → STOP

PHASE 완료 → cargo build --release → STOP
```

**핵심 파일 역할**:
| 파일 | 용도 | 갱신 시점 |
|------|------|----------|
| `.claude/execution-log.md` | 파이프라인 Step별 실행 이력 + 교훈 | 매 Step 완료/실패 시 |
| `PLAN.md` | Phase별 작업 체크리스트 | 작업 완료 시 `[x]` |
| `PROGRESS.md` | 날짜별 완료 기록 + 알려진 이슈 | 작업 완료 시 |
| `.claude/references/` | 기술 조사 자료 | Phase 첫 작업 시 |

### 사용 가능한 스킬 (Slash Commands)

| 스킬 | 용도 | 자동 호출 |
|------|------|----------|
| `/start-phase [N]` | 세션 시작, PLAN.md + PROGRESS.md 읽기, 다음 작업 안내 | O |
| `/finish-task [설명]` | 작업 완료, 빌드/테스트 + 추적 파일 업데이트 | X (수동) |
| `/build-test [--quick]` | cargo clippy → test → build --release | X (수동) |
| `/deploy [대상]` | 릴리스 빌드 → Docker 배포 → 헬스체크 | X (수동) |
| `/check-status` | systemd, Docker, OpenSearch 상태 확인 | O |
| `/verify-index` | OpenSearch 문서 수, 카테고리 분포, 인덱스 크기 | O |
| `/dashboard-check` | 대시보드 API 전체 엔드포인트 검증 | O |
| `/crawl-stats` | 크롤링 파일 수, 카테고리 분포, DB 통계 | O |
| `/opensearch-query [쿼리]` | OpenSearch 직접 쿼리 실행 | O |
| `/ref [키워드/Phase]` | references 디렉토리에서 조사 자료 검색 | O |
| `/add-ref [주제]` | 새 조사 자료 저장 + INDEX.md 업데이트 | X (수동) |
| `/crawl [옵션]` | 뉴스 크롤링 실행 | X (수동) |
| `/index [옵션]` | OpenSearch 인덱싱 실행 | X (수동) |
| `/search [쿼리]` | 벡터 검색 실행 | O |
| `/check-entities` | HTML 엔티티 잔존 여부 확인 | O |

---

## Project Overview
Rust 기반 네이버 뉴스 크롤러 + OpenSearch 벡터 검색 + LLM 온톨로지 시스템.
122,000+ 뉴스 기사, 40,000+ lines of Rust code (v0.1.6, edition 2021, MSRV 1.80).

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

### Current (v0.1)
```
crawl (Naver API → Markdown files)
  → index (Markdown → OpenSearch with embeddings)
  → ontology (LLM-based knowledge graph extraction)
  → serve (REST API + vector search)
```

### Target (v0.2) — 지식 레이어 자동화
```
┌─ Layer 1: 수집 ──────────────────────────────────────────────┐
│  crawl (Naver News API → Markdown)                           │
│  ├─ 30분마다 자동 크롤링 (systemd timer)                       │
│  ├─ 카테고리별 분류 (politics, economy, society, ...)          │
│  └─ 3-tier dedup (bloom → hashset → DB)                      │
├─ Layer 2: 인덱싱 + 임베딩 ───────────────────────────────────┤
│  index (Markdown → OpenSearch)                                │
│  ├─ 384-dim kNN 벡터 임베딩                                    │
│  ├─ Semantic chunking (기사 → 의미 단위 분할)         [TODO]   │
│  └─ Hybrid search (BM25 + kNN)                       [TODO]   │
├─ Layer 3: 지식 추출 ─────────────────────────────────────────┤
│  ontology (LLM → Knowledge Graph)                             │
│  ├─ 엔티티/관계 추출 (vLLM + Qwen2.5)                         │
│  ├─ Neo4j 지식그래프 저장                             [TODO]   │
│  ├─ 이벤트 감지 + 토픽 클러스터링                      [TODO]   │
│  └─ Temporal KG (시간축 이벤트 추적)                   [TODO]   │
├─ Layer 4: 검색 + 추론 ──────────────────────────────────────┤
│  serve (REST API)                                             │
│  ├─ GraphRAG (벡터 + 그래프 하이브리드 검색)           [TODO]   │
│  ├─ Cross-encoder reranking                           [TODO]   │
│  └─ LLM 답변 생성 (vLLM)                              [TODO]   │
└─ Layer 5: 시각화 ────────────────────────────────────────────┘
   dashboard (React → Barami)
   ├─ 뉴스 목록/검색/상세
   ├─ 통계 대시보드 (카테고리, 일별, 시간별)
   ├─ 이벤트 타임라인 뷰                                [TODO]
   └─ 지식그래프 탐색 뷰                                [TODO]
```

### Directory Structure
```
Baram/
├── src/                          # Rust 소스 코드
│   ├── main.rs                   #   CLI entry point (clap)
│   ├── models.rs                 #   ParsedArticle, NewsCategory, CrawlState
│   ├── error.rs                  #   Global error types
│   ├── commands/                 #   CLI 서브커맨드 핸들러
│   │   ├── index.rs              #     index: Markdown → OpenSearch
│   │   ├── ontology.rs           #     ontology: LLM 트리플 추출
│   │   ├── search.rs             #     search: 벡터 검색
│   │   └── serve.rs              #     serve: REST API 서버
│   ├── crawler/                  #   크롤링 엔진
│   │   ├── fetcher.rs            #     HTTP client (rate limiting)
│   │   ├── list.rs               #     뉴스 리스트 크롤러 (URL 수집)
│   │   ├── pipeline.rs           #     크롤 파이프라인 (fetch → parse → store)
│   │   ├── instance.rs           #     Crawler 인스턴스
│   │   ├── comment.rs            #     댓글 추출
│   │   ├── distributed.rs        #     분산 크롤링
│   │   └── url.rs                #     URL 유틸리티
│   ├── parser/                   #   HTML → 구조화 데이터
│   │   ├── html.rs               #     ArticleParser, parse_with_fallback()
│   │   ├── sanitize.rs           #     텍스트 정제, HTML 엔티티 디코딩
│   │   └── selectors.rs          #     CSS 셀렉터 (정적 캐시)
│   ├── embedding/                #   벡터 임베딩
│   │   ├── mod.rs                #     임베딩 서버, OpenSearch bulk 인덱싱
│   │   ├── tokenizer.rs          #     토크나이저
│   │   └── vectorize.rs          #     벡터 생성
│   ├── ontology/                 #   LLM 지식 추출
│   │   ├── extractor.rs          #     트리플 추출 (vLLM)
│   │   ├── linker.rs             #     엔티티 링킹
│   │   ├── storage.rs            #     온톨로지 저장
│   │   └── stats.rs              #     추출 통계
│   ├── storage/                  #   데이터 저장
│   │   ├── markdown.rs           #     Markdown/Handlebars 파일 쓰기
│   │   ├── checkpoint.rs         #     체크포인트 (JSON, atomic save)
│   │   ├── dedup.rs              #     3-tier 중복 제거 (bloom/hashset/DB)
│   │   └── repository.rs         #     SQLite 저장소
│   ├── config/                   #   설정 (TOML)
│   ├── llm/                      #   LLM 클라이언트 (vLLM API)
│   ├── analytics/                #   키워드/엔티티 트렌드 분석
│   ├── cache/                    #   캐시 레이어
│   ├── i18n/                     #   다국어 지원
│   ├── notifications/            #   알림 (webhook)
│   ├── scheduler/                #   분산 스케줄링
│   └── utils/                    #   에러, 재시도, 유틸리티
├── web/                          # 프론트엔드 (React + Vite)
│   └── src/
│       ├── pages/                #   Dashboard, Search, Ontology, Settings
│       ├── components/           #   Layout, StatCard, ErrorBoundary
│       ├── api/                  #   API 클라이언트
│       ├── hooks/                #   React Query 훅
│       └── types/                #   TypeScript 타입
├── tests/                        # 통합 테스트
│   ├── integration.rs
│   ├── parser_test.rs
│   └── fixtures/                 #   테스트 HTML 파일
├── docker/                       # Docker 인프라
│   ├── docker-compose.yml        #   메인 (PostgreSQL, OpenSearch, Redis, vLLM)
│   ├── docker-compose.monitoring.yml  # Prometheus + Grafana
│   ├── opensearch/               #   OpenSearch 설정, 인덱스 템플릿
│   ├── monitoring/               #   Prometheus 설정
│   └── vllm/                     #   vLLM 설정
├── scripts/                      # 자동화 스크립트
│   ├── crawl-only.sh             #   systemd 크롤링 (flock)
│   ├── index-only.sh             #   systemd 인덱싱 (flock)
│   ├── hourly-crawl.sh           #   연속 크롤링
│   └── fix-categories.py         #   카테고리 일괄 복구
├── output/                       # 데이터 출력 (gitignore)
│   ├── raw/                      #   크롤링된 Markdown 파일 (122,000+)
│   ├── crawl.db                  #   SQLite 크롤 메타데이터
│   ├── ontology.json             #   온톨로지 트리플 (JSON)
│   ├── ontology.ttl              #   온톨로지 (Turtle RDF)
│   └── checkpoints/              #   온톨로지 체크포인트
├── checkpoints/                  # 인덱싱 체크포인트
├── templates/                    # Handlebars 템플릿
│   └── article.hbs               #   기사 Markdown 템플릿
├── k8s/                          # Kubernetes 매니페스트
├── locales/                      # i18n 번역 파일
├── Cargo.toml                    # Rust 의존성
├── Dockerfile                    # CPU 빌드
├── Dockerfile.gpu                # GPU 빌드 (임베딩)
├── Makefile                      # 빌드/배포 명령
└── config.example.toml           # 설정 예시
```

### Key Modules
| Module | Path | Description |
|--------|------|-------------|
| Commands | `src/commands/` | CLI handlers (crawl, index, ontology, search, serve) |
| Crawler | `src/crawler/` | HTTP fetcher, pipeline, comment extractor, distributed |
| Embedding | `src/embedding/` | Vector generation, tokenizer, OpenSearch bulk indexing |
| Ontology | `src/ontology/` | LLM triple extraction, entity linking |
| Storage | `src/storage/` | SQLite, markdown writer, bloom filter dedup, checkpoint |
| Parser | `src/parser/` | HTML parsing, sanitization, CSS selectors |
| Config | `src/config/` | AppConfig with TOML support |
| LLM | `src/llm/` | vLLM API client |
| Web | `web/` | React dashboard (Vite + TypeScript) |

### Data Flow
- **Crawl output**: `./output/raw/{oid}_{aid}_{title}.md` (YAML frontmatter + markdown)
- **Database**: `./output/crawl.db` (SQLite - crawl metadata)
- **Checkpoints**: `./checkpoints/` (JSON - resumable state)
- **Ontology**: `./output/ontology.json` + `./output/ontology.ttl` (triples)
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
NEO4J_URL=bolt://localhost:7687          # [TODO] 지식그래프
NEO4J_AUTH=neo4j/password                # [TODO] 지식그래프
RUST_LOG=info  # or baram::crawler=debug
```

## Important Patterns
- **Incremental indexing**: Checkpoint pre-filtering by filename ID extraction (no file I/O)
- **Batch embedding**: `/embed/batch` endpoint (up to 100 texts per call)
- **3-tier dedup**: Bloom filter → HashSet cache → DB query
- **Parallel parsing**: `tokio::task::spawn_blocking` + `buffer_unordered`
- **Atomic checkpoint saves**: temp file + rename
- **Concurrent LLM**: `futures::stream::buffer_unordered(max_concurrent)` for ontology
- **Ontology checkpoint**: `./output/checkpoints/ontology_checkpoint.json` (resume on failure)
- **Lock-free embedding**: Atomic stats enable `&self` for embed — no RwLock needed
- **Category assignment**: `crawl_single_url()`에서 크롤 컨텍스트의 카테고리를 기사에 할당

---

## Roadmap: 지식 레이어 자동화

### Phase 1: Hybrid Search (난이도: 낮음, 영향도: 높음)
OpenSearch 설정 변경만으로 검색 품질 즉시 개선.

**목표**: BM25 키워드 검색 + kNN 벡터 검색을 결합한 하이브리드 쿼리

**구현 계획**:
1. OpenSearch 인덱스 매핑에 `text` 필드 + nori analyzer 추가 (이미 있음)
2. `src/commands/search.rs`에서 `hybrid` query 사용
   - `sub_queries`: BM25 (keyword match) + kNN (vector similarity)
   - `search_pipeline`: normalization + combination
3. OpenSearch search pipeline 생성:
   ```json
   PUT /_search/pipeline/hybrid-pipeline
   {
     "phase_results_processors": [{
       "normalization-processor": {
         "normalization": { "technique": "min_max" },
         "combination": { "technique": "arithmetic_mean", "parameters": { "weights": [0.3, 0.7] } }
       }
     }]
   }
   ```
4. 검색 API에 `mode=hybrid|vector|keyword` 파라미터 추가

**검증**: 동일 쿼리로 vector-only vs hybrid 결과 비교

### Phase 2: 이벤트 클러스터링 (난이도: 중간, 영향도: 높음)
같은 사건을 다룬 기사를 자동으로 그룹핑. 대시보드 가치 극대화.

**목표**: 뉴스 기사를 이벤트 단위로 자동 클러스터링

**구현 계획**:
1. 새 모듈 `src/commands/cluster.rs` + `src/clustering/` 생성
2. CLI: `baram cluster --input ./output/raw --since 2026-02-01`
3. 알고리즘:
   - 기사 임베딩을 OpenSearch에서 로드
   - HDBSCAN 또는 cosine similarity 기반 incremental clustering
   - 클러스터마다 vLLM으로 요약 + 이벤트 제목 생성
4. 결과 저장: `./output/clusters/` (JSON)
   ```json
   {
     "event_id": "evt_20260215_001",
     "title": "윤석열 탄핵 심판",
     "summary": "...",
     "articles": ["001_0015812889", "021_0002345678"],
     "first_seen": "2025-12-14",
     "last_updated": "2026-02-15",
     "category": "politics"
   }
   ```
5. API 엔드포인트: `GET /api/events`, `GET /api/events/:id`
6. 대시보드: 이벤트 타임라인 뷰

**검증**: 정치 카테고리 기사로 시범 클러스터링 → 수동 평가

### Phase 3: GraphRAG — Neo4j 지식그래프 (난이도: 중간, 영향도: 높음)
벡터 검색 + 지식그래프 탐색을 결합한 차세대 검색.

**목표**: 온톨로지 트리플을 Neo4j에 저장하고 GraphRAG 검색 지원

**구현 계획**:
1. Docker에 Neo4j 추가 (`docker-compose.yml`)
2. 새 모듈 `src/graphdb/` — Neo4j Bolt 클라이언트
3. 온톨로지 파이프라인 확장:
   - 기존: `기사 → vLLM → (S, P, O) 트리플 → JSON 파일`
   - 변경: `기사 → vLLM → (S, P, O) 트리플 → Neo4j + JSON 파일`
4. GraphRAG 검색 구현:
   ```
   query → [벡터 검색: OpenSearch kNN → top-50]
         → [그래프 검색: 엔티티 링킹 → Neo4j 서브그래프 추출]
         → Reciprocal Rank Fusion → top-10
         → vLLM 답변 생성 (context = 문서 + 그래프)
   ```
5. CLI: `baram search "query" --mode graphrag`
6. API: `GET /api/search?q=...&mode=graphrag`

**Neo4j 스키마**:
```cypher
(:Entity {name, type, aliases[]})
  -[:RELATED_TO {predicate, source_article, confidence, extracted_at}]->
(:Entity)

(:Article {id, title, category, published_at, url})
  -[:MENTIONS]->
(:Entity)

(:Event {id, title, summary, first_seen, last_updated})
  -[:INVOLVES]->
(:Entity)
  -[:COVERED_BY]->
(:Article)
```

**검증**: "윤석열 관련 인물" 쿼리 → 그래프에서 2-hop 관계 탐색 + 벡터 유사 기사 결합

### Phase 4: Semantic Chunking + Reranking (난이도: 중간, 영향도: 중간)
기사를 의미 단위로 분할하고, 검색 결과를 재정렬하여 정밀도 향상.

**목표**: 청크 단위 임베딩 + Cross-Encoder 리랭킹

**구현 계획**:
1. `src/embedding/chunker.rs` — 의미 기반 청킹
   - 문단/문장 경계 인식
   - 256-512 토큰, 10-20% 오버랩
   - 청크마다 문서 메타데이터 (제목, 카테고리) 주입
2. OpenSearch 인덱스 스키마 변경:
   - 기존: 기사 1개 = 문서 1개 (1 벡터)
   - 변경: 기사 1개 = 청크 N개 (N 벡터), `parent_id`로 원본 연결
3. 임베딩 서버에 reranking 엔드포인트 추가:
   - `POST /rerank` — Cross-Encoder 모델로 query-document 쌍 스코어링
   - 모델: `BAAI/bge-reranker-v2-m3` (다국어 지원)
4. 검색 파이프라인:
   ```
   query → hybrid search → top-50 청크 → cross-encoder rerank → top-10 → 원본 기사 매핑
   ```

**검증**: 기존 전체 임베딩 vs 청크 임베딩 + reranking A/B 비교

### Phase 5: Temporal Knowledge Graph (난이도: 높음, 영향도: 높음)
이벤트의 시간 흐름과 인과관계를 추적하는 시간축 지식그래프.

**목표**: 뉴스 이벤트의 발전 과정을 자동 추적하고 타임라인 생성

**구현 계획**:
1. 온톨로지 트리플에 시간 속성 추가:
   ```
   (윤석열, 탄핵소추, 국회) [when: 2024-12-14, event_id: evt_001]
   (헌법재판소, 심리개시, 탄핵) [when: 2024-12-16, event_id: evt_001]
   ```
2. Neo4j에 시간 관계 저장:
   ```cypher
   (:Event)-[:CAUSED]->(:Event)
   (:Event)-[:FOLLOWED_BY]->(:Event)
   (:Event {start_date, end_date, status: ongoing|resolved})
   ```
3. vLLM 프롬프트로 이벤트 간 인과관계 추론
4. API: `GET /api/events/:id/timeline` — 이벤트 타임라인 반환
5. 대시보드: 시간축 시각화 (D3.js timeline)

**검증**: 주요 정치 이벤트 1건의 타임라인 자동 생성 → 수동 검증

### Phase 6: 다중 에이전트 온톨로지 (난이도: 높음, 영향도: 중간)
온톨로지 추출 품질을 다중 에이전트 아키텍처로 향상.

**목표**: 전문화된 LLM 에이전트들이 협업하여 고품질 지식그래프 구축

**구현 계획**:
1. Agent 1 — 엔티티 추출: 인물, 조직, 장소, 날짜 NER
2. Agent 2 — 관계 분류: 엔티티 쌍에 대해 관계 유형 결정
3. Agent 3 — 스키마 매핑: 추출된 관계를 온톨로지 스키마에 정규화
4. Agent 4 — 품질 검증: 중복 제거, 충돌 해결, 신뢰도 점수
5. `src/ontology/agents/` 모듈 구조:
   ```
   src/ontology/
   ├── agents/
   │   ├── entity_extractor.rs
   │   ├── relation_classifier.rs
   │   ├── schema_mapper.rs
   │   └── quality_validator.rs
   ├── pipeline.rs    (에이전트 오케스트레이션)
   └── mod.rs
   ```
6. 각 에이전트는 독립 vLLM 프롬프트, 파이프라인으로 연결

**검증**: 단일 에이전트 vs 다중 에이전트 트리플 품질 비교 (precision/recall)

---

## Issue 작업 규칙

### 이슈 작업 시작 전
1. GitHub Issues에서 `ready` 라벨 확인
2. 관련 코드 탐색 (Explore agent 활용)
3. 구현 계획 수립 후 작업 시작

### 구현 후 체크리스트
- [ ] `cargo test` 전체 통과
- [ ] `cargo clippy` 경고 없음
- [ ] 관련 테스트 추가
- [ ] 기존 패턴 준수 (에러 처리, 로깅, checkpoint 등)
- [ ] 성능 영향 고려 (122,000+ 파일 규모)

### 커밋 메시지 형식
```
feat: 새 기능 추가
fix: 버그 수정
refactor: 코드 구조 개선
docs: 문서 업데이트
test: 테스트 추가/수정
chore: 설정, 빌드 등 기타
```
