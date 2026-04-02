#!/bin/bash
# Baram 뉴스 크롤링 스크립트 (카테고리 순환 + 자동 인덱싱 + AI 파이프라인)
# 30분마다 실행되며, 크롤링 후 자동으로 인덱싱 및 AI 처리 수행

set -e

LOG_DIR="/data/Baram/logs"
STATE_FILE="/data/Baram/scripts/.crawl_state"
CATEGORIES=(politics economy society culture world it)

# AI Gateway 설정
export LLM_ENDPOINT="http://10.100.3.30:8000"
export LLM_MODEL="qwen3.5-35b"
export LLM_TIMEOUT="120"
export LLM_MAX_TOKENS="4000"
export EMBEDDING_URL="http://10.100.3.30:8000/v1/embeddings"

mkdir -p "$LOG_DIR"

# 현재 카테고리 인덱스 로드 (없으면 0)
if [ -f "$STATE_FILE" ]; then
    CURRENT_INDEX=$(cat "$STATE_FILE")
else
    CURRENT_INDEX=0
fi

# 현재 카테고리 선택
CATEGORY="${CATEGORIES[$CURRENT_INDEX]}"

# 다음 인덱스 계산 및 저장
NEXT_INDEX=$(( (CURRENT_INDEX + 1) % ${#CATEGORIES[@]} ))
echo "$NEXT_INDEX" > "$STATE_FILE"

# 로그 파일 설정
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
LOG_FILE="$LOG_DIR/crawl_${CATEGORY}_${TIMESTAMP}.log"

echo "=== Baram Crawl Start ===" >> "$LOG_FILE"
echo "Time: $(date)" >> "$LOG_FILE"
echo "Category: $CATEGORY - index: $CURRENT_INDEX" >> "$LOG_FILE"
echo "AI Gateway: $LLM_ENDPOINT" >> "$LOG_FILE"
echo "=========================" >> "$LOG_FILE"

# Docker 크롤링 실행 (AI 환경 변수 포함)
cd /data/Baram
docker run --rm --network docker_baram-network \
    -e OPENSEARCH_URL=http://opensearch:9200 \
    -e LLM_ENDPOINT="$LLM_ENDPOINT" \
    -e LLM_MODEL="$LLM_MODEL" \
    -e LLM_TIMEOUT="$LLM_TIMEOUT" \
    -e LLM_MAX_TOKENS="$LLM_MAX_TOKENS" \
    -e EMBEDDING_URL="$EMBEDDING_URL" \
    -v /data/Baram/config.toml:/app/config.toml:ro \
    -v /data/Baram/output:/app/output \
    baram:latest crawl --category "$CATEGORY" --max-articles 100 \
    >> "$LOG_FILE" 2>&1

CRAWL_RESULT=$?

echo "" >> "$LOG_FILE"
echo "=== Crawl Finished ===" >> "$LOG_FILE"
echo "Exit code: $CRAWL_RESULT" >> "$LOG_FILE"
echo "Time: $(date)" >> "$LOG_FILE"

# 자동 인덱싱 실행 (임베딩 포함)
echo "" >> "$LOG_FILE"
echo "=== Auto Indexing Start ===" >> "$LOG_FILE"
echo "Time: $(date)" >> "$LOG_FILE"

docker run --rm --network docker_baram-network \
    -e OPENSEARCH_URL=http://opensearch:9200 \
    -e EMBEDDING_URL="$EMBEDDING_URL" \
    -v /data/Baram/config.toml:/app/config.toml:ro \
    -v /data/Baram/output:/app/output \
    baram:latest index --input /app/output/raw --batch-size 100 \
    >> "$LOG_FILE" 2>&1

INDEX_RESULT=$?

echo "" >> "$LOG_FILE"
echo "=== Indexing Finished ===" >> "$LOG_FILE"
echo "Exit code: $INDEX_RESULT" >> "$LOG_FILE"
echo "Time: $(date)" >> "$LOG_FILE"

# AI 파이프라인 실행 (개체/관계 추출)
echo "" >> "$LOG_FILE"
echo "=== AI Pipeline Start ===" >> "$LOG_FILE"
echo "Time: $(date)" >> "$LOG_FILE"

MAX_ARTICLES=10 timeout 600 /data/Baram/scripts/process-ai.sh >> "$LOG_FILE" 2>&1 || true

AI_RESULT=$?

echo "" >> "$LOG_FILE"
echo "=== AI Pipeline Finished ===" >> "$LOG_FILE"
echo "Exit code: $AI_RESULT" >> "$LOG_FILE"
echo "Time: $(date)" >> "$LOG_FILE"

# 오래된 로그 정리 (7일 이상)
find "$LOG_DIR" -name "crawl_*.log" -mtime +7 -delete 2>/dev/null || true
find "$LOG_DIR" -name "ai_process_*.log" -mtime +7 -delete 2>/dev/null || true

# 크롤링 또는 인덱싱 중 하나라도 실패하면 실패 반환
if [ $CRAWL_RESULT -ne 0 ] || [ $INDEX_RESULT -ne 0 ]; then
    exit 1
fi

exit 0
