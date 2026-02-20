---
name: dashboard-check
description: 대시보드의 모든 API 엔드포인트를 호출하여 응답을 검증합니다.
allowed-tools: Bash
---

Barami 대시보드 API 전체 엔드포인트를 검증합니다.

## 수행 단계

모든 API를 순차 호출하고 결과를 테이블로 정리:

```bash
echo "=== Dashboard API Health Check ==="
echo ""

endpoints=(
  "GET /api/health"
  "GET /api/stats"
  "GET /api/status"
  "GET /api/news?page=1&limit=5"
  "GET /api/news/search?q=경제&limit=5"
  "GET /api/stats/daily"
  "GET /api/categories"
)

for ep in "${endpoints[@]}"; do
  method=$(echo $ep | cut -d' ' -f1)
  path=$(echo $ep | cut -d' ' -f2)

  start=$(date +%s%N)
  status=$(curl -s -o /tmp/api_response.json -w "%{http_code}" "http://localhost:8080${path}")
  end=$(date +%s%N)
  elapsed=$(( (end - start) / 1000000 ))

  size=$(wc -c < /tmp/api_response.json)

  if [ "$status" = "200" ]; then
    result="OK"
  else
    result="FAIL"
  fi

  printf "%-35s %s  %4dms  %6d bytes\n" "$path" "$result" "$elapsed" "$size"
done
```

## 출력 형식

한국어로 결과를 요약:
- 전체 엔드포인트 수 / 성공 수
- 실패한 엔드포인트가 있으면 상세 에러 표시
- /api/stats 응답에서 총 기사 수, 오늘 기사 수 표시
