# Baram Progress Tracker

> 에이전트는 세션 시작 시 이 파일을 읽고 현재 상태를 파악합니다.
> 작업 완료/진행 시 이 파일을 업데이트합니다.

## 현재 상태: Phase 2 완료, Phase 3 대기

---

## 완료된 작업

### 2026-02-13: 증분 인덱싱 + Systemd 분리
- [x] `baram-crawl.timer` / `baram-index.timer` systemd 서비스 분리
- [x] `scripts/crawl-only.sh`, `scripts/index-only.sh` 생성 (flock 중복 방지)
- [x] `src/commands/index.rs` — 체크포인트 사전 필터링 (파일명 ID 추출, 파싱 없이 스킵)
- [x] `--since` CLI 옵션 추가 (파일 mtime 기반 필터링)
- [x] `extract_doc_id_from_filename()` 헬퍼 + 테스트
- [x] 인덱싱 시간: 6-7시간 → 수 분 (91% I/O 감소)

### 2026-02-14: HTML 엔티티 수정
- [x] `src/storage/markdown.rs` — Handlebars `no_escape` 등록 (3곳)
- [x] `src/parser/sanitize.rs` — `html_escape` crate로 교체
- [x] 기존 75,191개 파일 sed 일괄 치환
- [x] OpenSearch 강제 재인덱싱 완료

### 2026-02-15: 카테고리 빈 문자열 수정
- [x] 원인 분석: `crawl_single_url()`에서 카테고리 미전달
- [x] `src/commands/crawl.rs` — `category: Option<&NewsCategory>` 파라미터 추가
- [x] `scripts/fix-categories.py` — 네이버 메타태그에서 section ID 추출하여 복구
- [x] 카테고리 복구 결과: 72,603개 업데이트, 3,903개 기존, 45,956개 실패(삭제된 기사)
- [x] 릴리스 빌드 완료
- [ ] OpenSearch 강제 재인덱싱 (진행 중 — `reindex.log` 확인)

### 2026-02-15: 대시보드 연동 (Barami)
- [x] `news-api/src/search.rs` — `track_total_hits: true`, `get_dashboard_stats()` 추가
- [x] `news-api/src/routes/stats.rs` — OpenSearch aggregation 파싱
- [x] `news-api/src/models/stats.rs` — `StatsResponse`, `SystemStatusResponse`
- [x] `/api/status` 엔드포인트 추가 (DB, LLM, disk, uptime)
- [x] Docker 이미지 빌드 및 배포 완료
- [x] 대시보드 실시간 데이터 표시 확인

### 2026-02-15: CLAUDE.md 프로젝트 셋업
- [x] v0.2 Target Architecture (5-Layer) 정의
- [x] Roadmap Phase 1-6 구현 계획 작성
- [x] 디렉토리 구조 문서화
- [x] PROGRESS.md, PLAN.md 워크플로우 도입

### 2026-02-19: 자율 CI/CD 에이전트 구축
- [x] PM 에이전트 자동 루프 + 자동 커밋 + 에러 복구 파이프라인
- [x] Sub-agent 구조화 출력 프로토콜 (---RESULT--- 블록)
- [x] Implementer, Reviewer, Verifier, Researcher 에이전트
- [x] CLAUDE.md 자율 운영 모드 규칙 (STOP 조건, 자동 허가)
- [x] 재인덱싱 완료 확인 (125,002건, 카테고리 분포 정상)

### 2026-02-20: Phase 1 — Hybrid Search (진행 중)
- [x] OpenSearch hybrid-pipeline 생성 (min_max normalization, BM25:0.3 + kNN:0.7)
- [x] `search_hybrid()` native hybrid query 구현 (script_score → OpenSearch hybrid query type)
- [x] `execute_search_with_pipeline()` 추가 (opensearch-rs low-level transport)
- [x] CLI `--mode hybrid|keyword|vector` 파라미터 추가
- [x] `get_query_embedding()` — 임베딩 서버 연결
- [x] 코드 중복 제거: `parse_search_hits()`, `prepare_search_query()` 분리
- [x] HTTP 에러 처리 일관성 수정 (`execute_search()`)
- [x] UTF-8 경계 안전 문자열 자르기 수정
- [x] `src/commands/serve.rs` — `baram serve` CLI + `/api/search?mode=hybrid` REST API
- [x] GET /api/search: mode=hybrid|keyword|vector, k, threshold, category, date_from, date_to
- [x] GET /api/health: OpenSearch 연결 상태 + 문서 수 확인
- [x] reqwest::Client 공유 (ApiServerState), 7 unit tests

### 2026-02-21: Phase 2 -- Event Clustering
- [x] `src/clustering/mod.rs` -- 모듈 구조 (engine, models, summary)
- [x] `src/clustering/engine.rs` -- 코어 클러스터링 (cosine similarity, incremental centroid, search_after pagination)
- [x] `src/clustering/models.rs` -- EventCluster, ClusterArticle, ClusterOutput, ClusterConfig, ClusterMetadata
- [x] `src/clustering/summary.rs` -- vLLM 기반 이벤트 요약 생성
- [x] `src/commands/cluster.rs` -- `baram cluster` CLI (--category, --since, --threshold, --min-size, --summarize)
- [x] `src/commands/serve.rs` -- GET /api/events (pagination, category filter), GET /api/events/:id
- [x] `src/embedding/mod.rs` -- VectorStore::raw_search() 추가
- [x] 16 new tests (engine 5, models 4, summary 3, cluster 2, serve 2)

---

## 진행 중

### Phase 2 완료 (2026-02-21)
- 전체 구현 및 테스트 완료
- 다음: release build + Phase 3 진행 여부 확인

---

## 알려진 이슈
- 삭제된 기사 45,956개는 카테고리 복구 불가 (네이버에서 삭제됨)
- `cargo clippy`에서 deprecated 경고 2개 (`korean_desc` → `localized_desc`)
