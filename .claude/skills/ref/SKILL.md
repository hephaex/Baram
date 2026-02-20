---
name: ref
description: .claude/references/ 디렉토리에서 관련 참고 자료를 검색하고 읽습니다. Phase 작업이나 기술 조사 시 자동으로 사용합니다.
argument-hint: [검색 키워드 또는 Phase 번호]
allowed-tools: Read, Grep, Glob
---

참고 자료를 검색하고 관련 내용을 제공합니다.

검색어: $ARGUMENTS

## 수행 단계

1. **인덱스 읽기**: `.claude/references/INDEX.md`를 읽어 파일 목록과 키워드 매핑 확인

2. **검색**:
   - Phase 번호가 지정되면: 해당 Phase 관련 파일을 INDEX.md에서 찾아 읽기
   - 키워드가 지정되면: INDEX.md의 키워드 매핑에서 관련 파일 찾기
   - 매핑에 없으면: `.claude/references/*.md` 파일에서 Grep으로 키워드 검색

3. **관련 파일 읽기**: 찾은 참고 자료 파일을 읽고 핵심 내용 요약

4. **출력**: 한국어로 관련 내용 요약 + 원본 파일 경로 안내
