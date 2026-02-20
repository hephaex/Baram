---
name: opensearch-query
description: OpenSearch에 직접 쿼리를 실행합니다. 디버깅, 검증, 탐색에 사용합니다.
argument-hint: [search query text or JSON body]
allowed-tools: Bash
---

OpenSearch에 쿼리를 실행하고 결과를 포맷팅합니다.

Arguments: $ARGUMENTS

## 동작 방식

### 텍스트 쿼리인 경우
인자가 일반 텍스트면 multi_match 검색:
```bash
curl -s 'http://localhost:9200/baram-articles/_search' \
  -H 'Content-Type: application/json' \
  -d '{
    "size": 5,
    "track_total_hits": true,
    "query": {
      "multi_match": {
        "query": "$ARGUMENTS",
        "fields": ["title^3", "content", "publisher"]
      }
    },
    "_source": ["title", "category", "publisher", "published_at", "url"]
  }' | python3 -m json.tool
```

### JSON 바디인 경우
인자가 `{`로 시작하면 그대로 OpenSearch에 전달:
```bash
curl -s 'http://localhost:9200/baram-articles/_search' \
  -H 'Content-Type: application/json' \
  -d '$ARGUMENTS' | python3 -m json.tool
```

## 출력 형식

결과를 한국어로 정리:
- 총 매칭 수
- 상위 결과를 테이블 형식으로 (제목, 카테고리, 언론사, 날짜)
