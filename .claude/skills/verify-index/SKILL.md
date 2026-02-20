---
name: verify-index
description: OpenSearch 인덱스 상태를 검증합니다. 문서 수, 카테고리 분포, 최근 인덱싱 시간을 확인합니다.
allowed-tools: Bash
---

OpenSearch 인덱스 상태를 종합 검증합니다.

## 수행 단계

1. **문서 수 확인**:
   ```bash
   curl -s 'http://localhost:9200/baram-articles/_count' | python3 -m json.tool
   ```

2. **카테고리 분포**:
   ```bash
   curl -s 'http://localhost:9200/baram-articles/_search' \
     -H 'Content-Type: application/json' \
     -d '{"size":0,"track_total_hits":true,"aggs":{"categories":{"terms":{"field":"category","size":20}}}}' \
     | python3 -c "
   import sys, json
   d = json.load(sys.stdin)
   total = d['hits']['total']['value']
   print(f'Total documents: {total}')
   print('Category distribution:')
   for b in d['aggregations']['categories']['buckets']:
       pct = b['doc_count']/total*100
       print(f'  {b[\"key\"]:15s} {b[\"doc_count\"]:>7,} ({pct:.1f}%)')
   empty = total - sum(b['doc_count'] for b in d['aggregations']['categories']['buckets'])
   if empty > 0:
       print(f'  {\"(empty)\":15s} {empty:>7,} ({empty/total*100:.1f}%)')
   "
   ```

3. **최근 인덱싱 확인** (최근 24시간):
   ```bash
   curl -s 'http://localhost:9200/baram-articles/_search' \
     -H 'Content-Type: application/json' \
     -d '{"size":0,"query":{"range":{"crawled_at":{"gte":"now-24h"}}},"track_total_hits":true}' \
     | python3 -c "import sys,json; print(f'Last 24h indexed: {json.load(sys.stdin)[\"hits\"][\"total\"][\"value\"]}')"
   ```

4. **인덱스 크기**:
   ```bash
   curl -s 'http://localhost:9200/baram-articles/_stats/store' \
     | python3 -c "import sys,json; s=json.load(sys.stdin)['indices']['baram-articles']['total']['store']['size_in_bytes']; print(f'Index size: {s/1024/1024:.1f} MB')"
   ```

한국어로 결과를 요약 테이블로 보고.
