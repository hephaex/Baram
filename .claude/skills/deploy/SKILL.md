---
name: deploy
description: Baram 릴리스 빌드 → Docker 이미지 빌드 → 컨테이너 재시작 → 헬스체크 순서로 배포합니다.
disable-model-invocation: true
argument-hint: [all | baram | news-api | dashboard]
allowed-tools: Bash, Read
---

Baram 시스템을 빌드하고 배포합니다.

대상: $ARGUMENTS (기본값: all)

## 수행 단계

### 1. Baram (Rust CLI) 배포
대상이 `all` 또는 `baram`이면:
```bash
cd /home/mare/Baram
cargo build --release 2>&1 | tail -5
```
빌드 실패 시 에러를 보고하고 중단.

### 2. News API (Docker) 배포
대상이 `all` 또는 `news-api`이면:
```bash
cd /home/mare/Barami
docker compose build barami-news-api 2>&1 | tail -10
docker compose up -d barami-news-api 2>&1
sleep 5
curl -sf http://localhost:8080/api/health && echo "OK" || echo "FAILED"
```

### 3. Dashboard (Docker) 배포
대상이 `all` 또는 `dashboard`이면:
```bash
cd /home/mare/Barami
docker compose build barami-news-dashboard 2>&1 | tail -10
docker compose up -d barami-news-dashboard 2>&1
sleep 3
curl -sf http://localhost:3001/ > /dev/null && echo "OK" || echo "FAILED"
```

### 4. 헬스체크
```bash
echo "=== Health Check ==="
echo -n "News API: "  && curl -sf http://localhost:8080/api/health | python3 -m json.tool 2>/dev/null || echo "FAILED"
echo -n "OpenSearch: " && curl -sf http://localhost:9200/_cluster/health | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['status'])" 2>/dev/null || echo "FAILED"
echo -n "Embedding: " && curl -sf http://localhost:8090/health && echo "" || echo "FAILED"
```

### 5. 결과 보고
한국어로 빌드/배포 결과를 요약 보고.
