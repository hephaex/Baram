---
name: add-ref
description: 새로운 조사 자료를 .claude/references/에 저장하고 INDEX.md를 업데이트합니다.
disable-model-invocation: true
argument-hint: [주제명]
---

새로운 참고 자료를 references에 저장합니다.

주제: $ARGUMENTS

## 수행 단계

1. **파일명 결정**: 주제를 영문 kebab-case로 변환 (예: "뉴스 분류 모델" → `news-classification-model.md`)

2. **파일 생성**: `.claude/references/{파일명}.md` 생성
   - 상단에 메타 정보:
     ```
     # [주제명]
     > 조사일: [오늘 날짜]
     > 관련 Phase: [해당 Phase 번호]
     ```
   - 핵심 개념, 구현 방법, 성능 수치
   - 참고 자료 URL 목록

3. **INDEX.md 업데이트**:
   - 파일 목록 테이블에 새 행 추가
   - 키워드 → 파일 매핑에 관련 키워드 추가

4. **확인**: 추가된 자료 요약 출력
