# Execution Log

> PM 에이전트가 매 파이프라인 실행마다 이 파일에 기록합니다.
> 새 세션 시작 시 이 파일을 읽어 이전 이력을 파악합니다.
> 최근 3개 세션만 유지하고, 오래된 것은 `execution-log-archive.md`로 이동합니다.

---

## Session: 2026-02-20 — Phase 1: Hybrid Search 구현

### 파이프라인 실행 결과
| Step | Agent | 결과 | 상세 |
|------|-------|------|------|
| 커밋 | PM | ✅ | 에이전트 시스템 + 코드 수정 커밋 완료 |
| 조사 (A) | researcher | ✅ | OpenSearch 3.4 hybrid search: native hybrid query + body search_pipeline 방식 확인 |
| 구현 (B) | implementer | ✅ | search_hybrid(), CLI --mode, get_query_embedding() 구현 |
| 리뷰 (C) | reviewer | FIX (3건) | HTTP 에러 미처리, 코드 중복, UTF-8 경계 |
| 수정 (B2) | PM 직접 | ✅ | 3건 모두 수정: parse_search_hits() 추출, status 체크, char_indices() |
| 검증 (D) | verifier | ✅ | clippy 0 error, 613 tests passed (config_test 2 기존 실패 무관) |
| 커밋 (E) | PM | ✅ | feat: Add OpenSearch native hybrid search |

### 핵심 발견
- **OpenSearch 3.x에서 `cosineSimilarity` painless 함수 compile error** → 기존 `search_hybrid()` 완전히 깨져있었음
- `opensearch-rs 2.3`에 `search_pipeline` 빌더 미지원 → low-level `send()` API 사용
- `search_pipeline`을 body JSON 필드로 넣어도 동작 (URL param과 동일)

### 교훈
- OpenSearch 메이저 버전 업그레이드 시 painless 스크립트 호환성 반드시 확인
- 코드 중복은 리뷰 단계에서 잡히므로 구현 시 빠른 속도 우선 → 리뷰에서 정리
- UTF-8 경계 문제는 한국어 데이터에서 항상 발생 가능 → `&str[..n]` 절대 사용 금지

### 다음 해야 할 일
1. **Phase 1 마무리**: serve.rs `/api/search?mode=hybrid` API 엔드포인트 추가
2. **Phase 1 완료 후**: cargo build --release → Phase 2 시작 여부 STOP

---

## Session: 2026-02-19 — 에이전트 시스템 구축

### 완료된 작업
| 작업 | 상태 | 상세 |
|------|------|------|
| PM 에이전트 자동 루프 | ✅ | auto-loop, auto-commit, 3단계 에러복구, Context Guard |
| Sub-agent 출력 표준화 | ✅ | 5개 에이전트 `---RESULT---` 블록 적용 |
| Context Guard (/compact) | ✅ | 2개 작업마다 checkpoint_and_compact() |
| CLAUDE.md 자율 운영 규칙 | ✅ | Context Guard, 에러복구, 프로토콜 반영 |

### 미커밋 변경 파일
- `.claude/agents/pm.md` — 자율 PM (자동 루프 + /compact + 에러 복구)
- `.claude/agents/implementer.md` — Rust 구현 에이전트 (---RESULT---)
- `.claude/agents/reviewer.md` — 코드 리뷰 에이전트 (PASS/FIX/BLOCK)
- `.claude/agents/verifier.md` — 빌드/테스트 검증 에이전트
- `.claude/agents/researcher.md` — 기술 조사 에이전트
- `.claude/execution-log.md` — 이 파일
- `CLAUDE.md` — 자율 운영 규칙 추가
- `PLAN.md`, `PROGRESS.md` — 상태 갱신
- `src/parser/sanitize.rs`, `src/storage/markdown.rs` — 이전 수정 (HTML 엔티티)
- `scripts/index-only.sh` — 이전 수정

### 다음 해야 할 일
1. **미커밋 변경사항 커밋** (에이전트 시스템 + 이전 코드 수정)
2. **Phase 1 시작**: PLAN.md 첫 `[ ]` = "OpenSearch hybrid search pipeline 생성"

### 교훈
- Sub-agent는 반드시 `---RESULT---` 블록으로 결과를 반환해야 PM이 자동 파싱 가능
- PM의 /compact 주기는 2개 작업마다 — 3개 이상 기다리면 이력 손실 위험
- execution-log.md + PLAN.md + PROGRESS.md 3개 파일로 세션 간 상태 복원
