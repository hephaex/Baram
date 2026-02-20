---
name: crawl-stats
description: 크롤링 통계를 확인합니다. 파일 수, 카테고리 분포, 오늘 크롤 건수, DB 통계를 보여줍니다.
allowed-tools: Bash
---

크롤링 현황을 종합 확인합니다.

## 수행 단계

1. **파일 수 확인**:
   ```bash
   echo "Total markdown files: $(find /home/mare/Baram/output/raw -name '*.md' | wc -l)"
   ```

2. **오늘 크롤링된 파일**:
   ```bash
   echo "Today's files: $(find /home/mare/Baram/output/raw -name '*.md' -newer /home/mare/Baram/output/raw -mtime -1 | wc -l)"
   ```

3. **카테고리 분포** (파일 기준):
   ```bash
   find /home/mare/Baram/output/raw -name '*.md' -print0 \
     | xargs -0 grep -h '^category:' \
     | sort | uniq -c | sort -rn
   ```

4. **SQLite DB 통계**:
   ```bash
   python3 -c "
   import sqlite3
   conn = sqlite3.connect('/home/mare/Baram/output/crawl.db')
   c = conn.cursor()
   c.execute('SELECT COUNT(*) FROM crawl_metadata')
   total = c.fetchone()[0]
   c.execute(\"SELECT status, COUNT(*) FROM crawl_metadata GROUP BY status\")
   stats = dict(c.fetchall())
   print(f'DB Total: {total}')
   for k, v in stats.items():
       print(f'  {k}: {v} ({v/total*100:.1f}%)')
   conn.close()
   " 2>/dev/null || echo "SQLite not available"
   ```

5. **디스크 사용량**:
   ```bash
   du -sh /home/mare/Baram/output/raw/ /home/mare/Baram/output/crawl.db 2>/dev/null
   ```

한국어로 요약 보고.
