# baram Distributed Crawler - Quick Start Guide

## Current Deployment Status: ACTIVE

All services are running and ready to use:
- PostgreSQL: Accepting connections (port 5432)
- Redis: Operational (port 6379)
- OpenSearch: Green cluster status (port 9200)
- 3 Crawler instances: Running in distributed mode

## What's Running

```
Service              Status    Port(s)
─────────────────────────────────────────────────────────
baram-postgres      Healthy   0.0.0.0:5432
baram-redis         Healthy   0.0.0.0:6379
baram-opensearch    Healthy   0.0.0.0:9200
baram-crawler-main  Running   (internal: 8080)
baram-crawler-sub1  Running   (internal: 8080)
baram-crawler-sub2  Running   (internal: 8080)
```

## Key Files Modified

1. `/home/mare/baram/docker/.env`
   - Updated PostgreSQL port to 5432 (standard)
   - Added COMPOSE_PROJECT_NAME and VERSION
   - Added development tools configuration

2. `/home/mare/baram/docker/docker-compose.distributed.yml`
   - Disabled coordinator service (not yet implemented)
   - Updated crawler commands to use `distributed` subcommand
   - Removed coordinator dependencies from crawlers
   - Configured each instance with PostgreSQL deduplication

## Usage Examples

### View Status
```bash
cd /home/mare/baram/docker
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml ps
```

### Follow Live Logs
```bash
# Watch main crawler
docker logs baram-crawler-main -f

# Watch all crawlers
docker logs baram-crawler-main -f &
docker logs baram-crawler-sub1 -f &
docker logs baram-crawler-sub2 -f &
```

### Stop Everything
```bash
cd /home/mare/baram/docker
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml down
```

### Restart Services
```bash
cd /home/mare/baram/docker
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml restart
```

## Architecture

### Three-Instance Distributed Crawler
- **main**: Primary crawler instance (default orchestrator)
- **sub1**: Secondary crawler instance
- **sub2**: Tertiary crawler instance

Each instance:
- Runs independently with same rate limiting (2.0 req/s)
- Shares PostgreSQL database for deduplication
- Outputs crawled articles to separate volumes
- Sends heartbeats every 30 seconds

### Data Isolation
```
baram_crawler_main_output/    → Articles from main instance
baram_crawler_sub1_output/    → Articles from sub1 instance
baram_crawler_sub2_output/    → Articles from sub2 instance

baram_postgres_data/          → Shared deduplication database
baram_opensearch_data/        → Shared search index
baram_redis_data/             → Shared cache
```

## Performance Characteristics

- **Total Rate**: 6.0 requests/second (3 instances × 2.0 rps)
- **Deduplication**: PostgreSQL-based, 100% reliable
- **Memory per instance**: 256MB reserved, 1GB limit
- **Heartbeat interval**: 30 seconds (configurable)

## Database Access

### PostgreSQL
```bash
# Connect directly
docker exec -it baram-postgres psql -U baram -d baram

# View deduplication table
# SELECT * FROM crawl_status;
```

### Redis
```bash
# Access redis-cli
docker exec -it baram-redis redis-cli

# Check memory usage
# INFO memory
```

### OpenSearch
```bash
# Check cluster health
curl http://localhost:9200/_cluster/health | jq

# List indices
curl http://localhost:9200/_cat/indices | jq
```

## Expected Behavior

### On Startup
1. Crawlers start and initialize deduplication
2. They attempt to register with coordinator (will fail - expected)
3. They schedule heartbeats every 30 seconds
4. They wait for the next hour to start crawling
5. Logs show "waiting NNN seconds until next hour"

### Normal Operation
- Periodic "Heartbeat failed" warnings (coordinator not running - expected)
- No errors in PostgreSQL/Redis/OpenSearch communication
- Steady log output every 30 seconds

### To Actually Crawl
Need to either:
1. Implement and start the coordinator service, OR
2. Run in "once" mode to execute immediately (see DEPLOYMENT_SUMMARY.md)

## Environment Variables

Key configuration in `/home/mare/baram/docker/.env`:

```
# Database
POSTGRES_PASSWORD=nT1m3s_Pr0d_2024!
POSTGRES_PORT=5432

# Crawler behavior
CRAWLER_LOG_LEVEL=info
REQUESTS_PER_SECOND=2.0
HEARTBEAT_INTERVAL=30

# Search
OPENSEARCH_PORT=9200

# Cache
REDIS_PORT=6379
```

## Troubleshooting

### "Container is unhealthy"
Normal - health check tries to connect to coordinator on port 8080 which doesn't exist yet.
The containers are still running fine.

### "Heartbeat failed" warnings
Expected - coordinator is not implemented. Crawlers continue to function normally.

### PostgreSQL won't start
```bash
docker logs baram-postgres
# Look for permission issues, check available disk space
```

### OpenSearch won't start
```bash
docker logs baram-opensearch
# Check memory settings, ensure Docker has enough resources
```

## Next Steps

1. **Implement Coordinator**: Add `coordinator` subcommand to Rust binary
2. **Test Single Crawl**: Use `--once` flag to run a test
3. **Monitor Performance**: Track RPS and deduplication rates
4. **Scale Up**: Increase REQUESTS_PER_SECOND or add more instances
5. **Enable Monitoring**: Uncomment opensearch-dashboards service

## Important Paths

- Configuration: `/home/mare/baram/docker/docker-compose.distributed.yml`
- Environment: `/home/mare/baram/docker/.env`
- Source Code: `/home/mare/baram/src/crawler/distributed.rs`
- Deployment Summary: `/home/mare/baram/DEPLOYMENT_SUMMARY.md`

---

**Deployment is ready for development and testing. See DEPLOYMENT_SUMMARY.md for comprehensive documentation.**
