---
name: start-phase
description: 세션 시작 시 PLAN.md와 PROGRESS.md를 읽고 현재 상태와 다음 작업을 파악합니다. 새 세션이 시작될 때 자동으로 사용합니다.
argument-hint: [phase number (optional)]
allowed-tools: Read, Bash, Grep, Glob
---

세션을 시작합니다. 현재 프로젝트 상태를 파악하고 다음 작업을 안내합니다.

Arguments: $ARGUMENTS

## 수행 단계

1. **PLAN.md 읽기** — `PLAN.md`를 읽고 "즉시 해야 할 일" 섹션을 확인
2. **PROGRESS.md 읽기** — `PROGRESS.md`를 읽고 "진행 중" 섹션과 "알려진 이슈" 확인
3. **백그라운드 프로세스 확인**:
   ```bash
   ps aux | grep -E 'baram|fix-categories|reindex' | grep -v grep
   ```
4. **로그 확인** (진행 중인 작업이 있으면):
   - `tail -3 ~/Baram/reindex.log 2>/dev/null`
   - `tail -3 ~/Baram/fix-categories.log 2>/dev/null`
5. **Systemd 타이머 확인**:
   ```bash
   systemctl --user list-timers --all 2>/dev/null | grep baram
   ```
6. **특정 Phase가 지정되었으면** CLAUDE.md의 해당 Roadmap Phase 섹션 읽기

## 출력 형식

한국어로 아래 형식의 요약을 출력:

```
## 현재 상태
- 진행 중: [진행 중인 작업]
- 백그라운드: [실행 중인 프로세스]
- 알려진 이슈: [있으면 표시]

## 다음 작업
- [PLAN.md의 "즉시 해야 할 일" 내용]

## Phase [N] 체크리스트
- [ ] 미완료 항목들
- [x] 완료 항목들
```
