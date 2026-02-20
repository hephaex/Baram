# Baram Development Plan

> 에이전트는 세션 시작 시 이 파일을 읽고 다음 작업을 결정합니다.
> 각 Phase의 세부 구현 계획은 CLAUDE.md의 Roadmap 섹션을 참조합니다.

## 현재 Phase: 재인덱싱 완료 대기 → Phase 1 시작

---

## 즉시 해야 할 일 (Next Actions)

### Phase 1: Hybrid Search 시작
- 재인덱싱 완료 확인됨 (125,002건)
- 선행 커밋: v0.1 안정화 변경사항 (crawl.rs, CLAUDE.md, PLAN.md, PROGRESS.md, .claude/)
- Phase 1 첫 작업: OpenSearch hybrid search pipeline 생성

---

## Phase 별 계획

### Phase 1: Hybrid Search [다음]
- **난이도**: 낮음 | **영향도**: 높음
- **예상 소요**: 1-2일
- **선행 조건**: 재인덱싱 완료
- **세부 계획**: CLAUDE.md → Roadmap → Phase 1
- **작업 항목**:
  - [ ] OpenSearch hybrid search pipeline 생성
  - [ ] `src/commands/search.rs` — hybrid query 모드 추가
  - [ ] `src/commands/serve.rs` — `/api/search?mode=hybrid` 파라미터
  - [ ] 벡터 vs 하이브리드 검색 결과 비교 검증
  - [ ] 테스트 추가

### Phase 2: 이벤트 클러스터링 [대기]
- **난이도**: 중간 | **영향도**: 높음
- **예상 소요**: 3-5일
- **선행 조건**: Phase 1 완료
- **세부 계획**: CLAUDE.md → Roadmap → Phase 2
- **작업 항목**:
  - [ ] `src/clustering/` 모듈 생성
  - [ ] `src/commands/cluster.rs` CLI 커맨드
  - [ ] 임베딩 기반 cosine similarity 클러스터링
  - [ ] vLLM 클러스터 요약 생성
  - [ ] `./output/clusters/` JSON 출력
  - [ ] API 엔드포인트 (`/api/events`)
  - [ ] 대시보드 이벤트 뷰

### Phase 3: GraphRAG + Neo4j [대기]
- **난이도**: 중간 | **영향도**: 높음
- **선행 조건**: Phase 2 완료
- **세부 계획**: CLAUDE.md → Roadmap → Phase 3
- **작업 항목**:
  - [ ] Docker Compose에 Neo4j 추가
  - [ ] `src/graphdb/` 모듈 (Bolt 클라이언트)
  - [ ] 온톨로지 → Neo4j 저장 파이프라인
  - [ ] GraphRAG 검색 (`--mode graphrag`)
  - [ ] Reciprocal Rank Fusion 구현

### Phase 4: Semantic Chunking + Reranking [대기]
- **세부 계획**: CLAUDE.md → Roadmap → Phase 4

### Phase 5: Temporal Knowledge Graph [대기]
- **세부 계획**: CLAUDE.md → Roadmap → Phase 5

### Phase 6: 다중 에이전트 온톨로지 [대기]
- **세부 계획**: CLAUDE.md → Roadmap → Phase 6

---

## 작업 규칙 (에이전트용)

### 세션 시작 시 (PM 에이전트)
1. `.claude/execution-log.md` 읽기 — 이전 실행 이력, 중단 지점, 교훈
2. `PLAN.md` 읽기 — 미완료 체크박스
3. `PROGRESS.md` 읽기 — 알려진 이슈
4. 중단된 작업 있으면 이어서 진행, 없으면 첫 `[ ]` 항목 시작

### 작업 완료 시
1. `.claude/execution-log.md`에 파이프라인 테이블 + 교훈 기록
2. `PROGRESS.md`에 완료 항목 기록
3. `PLAN.md`에서 체크박스 체크 + "즉시 해야 할 일" 갱신
4. **즉시 다음 작업으로 자동 진행** (STOP 조건이 아닌 한)

### 세션 간 인수인계
- 중단 시: execution-log.md에 중단 Step + 에러 상세 기록
- 재시작 시: execution-log.md 읽어서 중단 지점부터 재개
- sub-agent에게 이전 이력 전달하여 같은 실수 반복 방지
