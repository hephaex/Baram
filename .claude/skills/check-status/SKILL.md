---
name: check-status
description: Check the status of Baram systemd services, timers, and running processes. Use when the user asks about service status, indexing progress, or system health.
allowed-tools: Bash
---

Check the status of all Baram services and report a concise summary:

1. **Systemd timers** - Run `systemctl --user list-timers --all | grep baram` to show next trigger times
2. **Systemd services** - Run `systemctl --user status baram-crawl baram-index baram-embedding --no-pager 2>&1 | head -40`
3. **Running processes** - Run `ps aux | grep 'baram' | grep -v grep` to check active baram processes
4. **Index lock** - Check if `.index.lock` or `.crawl.lock` exists
5. **Recent logs** - Show last 10 lines from `logs/` directory: `tail -5 logs/crawl-$(date +%Y%m%d).log logs/index-$(date +%Y%m%d).log 2>/dev/null`
6. **OpenSearch doc count** - Run `curl -s localhost:9200/baram-articles/_count 2>/dev/null | python3 -m json.tool 2>/dev/null || echo "OpenSearch not reachable"`

Report results in a clear summary table format in Korean.
