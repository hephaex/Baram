---
name: finish-task
description: 작업 완료 시 빌드/테스트 확인 후 PROGRESS.md와 PLAN.md를 업데이트합니다.
disable-model-invocation: true
argument-hint: [완료한 작업 설명]
---

작업을 마무리하고 추적 파일을 업데이트합니다.

완료한 작업: $ARGUMENTS

## 수행 단계

1. **빌드 확인**:
   ```bash
   cargo clippy 2>&1 | tail -5
   cargo test 2>&1 | tail -10
   ```
   - 실패하면 문제를 먼저 수정한 후 계속 진행

2. **PROGRESS.md 업데이트**:
   - "완료된 작업" 섹션에 오늘 날짜로 항목 추가
   - 변경한 파일 목록과 결과를 기록
   - "진행 중" 섹션에서 완료된 항목 제거

3. **PLAN.md 업데이트**:
   - 완료된 체크박스를 `[x]`로 변경
   - "즉시 해야 할 일" 섹션을 다음 작업으로 갱신
   - 모든 항목이 완료되면 Phase 상태를 "완료"로 변경

4. **변경 사항 요약 출력** (한국어):
   - 완료된 작업
   - 변경된 파일
   - 다음 작업 안내

5. **커밋 여부 확인**: 사용자에게 커밋할지 물어봄
