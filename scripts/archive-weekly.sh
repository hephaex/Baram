#!/bin/bash
# Baram 주간 아카이브 스크립트
# 매주 일요일 01:00 실행

set -e

RAW_DIR="/data/Baram/output/raw"
ARCHIVE_DIR="/data/Baram/archives"
LOG_DIR="/data/Baram/logs"

mkdir -p "$ARCHIVE_DIR" "$LOG_DIR"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
YEAR=$(date +%Y)
WEEK=$(date +%V)
ARCHIVE_NAME="${YEAR}_${WEEK}주차_${TIMESTAMP}.tar.gz"
LOG_FILE="$LOG_DIR/archive_${TIMESTAMP}.log"

echo "=== Baram Archive Start ===" >> "$LOG_FILE"
echo "Time: $(date)" >> "$LOG_FILE"
echo "Archive: $ARCHIVE_NAME" >> "$LOG_FILE"
echo "===========================" >> "$LOG_FILE"

# md 파일 개수 확인
MD_COUNT=$(find "$RAW_DIR" -maxdepth 1 -name "*.md" -type f 2>/dev/null | wc -l)
echo "Found $MD_COUNT markdown files" >> "$LOG_FILE"

if [ "$MD_COUNT" -eq 0 ]; then
    echo "No markdown files to archive. Exiting." >> "$LOG_FILE"
    exit 0
fi

# find + tar 사용 (파일 수 제한 우회)
cd "$RAW_DIR"
find . -maxdepth 1 -name "*.md" -type f -print0 | tar -czvf "$ARCHIVE_DIR/$ARCHIVE_NAME" --null -T - >> "$LOG_FILE" 2>&1

RESULT=$?

if [ $RESULT -eq 0 ]; then
    # 아카이브 성공 시 원본 삭제
    #DISABLED: find ... -delete (keep files after archive)
    echo "Successfully archived and removed $MD_COUNT files" >> "$LOG_FILE"
    
    # 아카이브 크기 기록
    ARCHIVE_SIZE=$(du -h "$ARCHIVE_DIR/$ARCHIVE_NAME" | cut -f1)
    echo "Archive size: $ARCHIVE_SIZE" >> "$LOG_FILE"
else
    echo "Archive failed with exit code: $RESULT" >> "$LOG_FILE"
fi

echo "=== Archive Finished ===" >> "$LOG_FILE"
echo "Time: $(date)" >> "$LOG_FILE"

# 오래된 아카이브 정리 (90일 이상)
#DISABLED: find ... -delete (keep files after archive) 2>/dev/null || true

# PROGRESS.md 업데이트
if [ -f /data/Baram/scripts/update-progress.sh ]; then
    /data/Baram/scripts/update-progress.sh archive 0 성공 "아카이브: $ARCHIVE_NAME ($ARCHIVE_SIZE)" 2>/dev/null || true
fi

exit $RESULT
