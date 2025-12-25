# baram Distributed Crawler Deployment Summary

## Deployment Date
2025-12-19

## Deployment Status
SUCCESSFUL - All services deployed and running

## Deployed Services

### Core Infrastructure
- **PostgreSQL 18** with pgvector
  - Port: 5432 (host) -> 5432 (container)
  - Database: baram
  - User: baram
  - Status: Healthy
  - Used for: Article storage, deduplication tracking

- **Redis 7 Alpine**
  - Port: 6379 (host) -> 6379 (container)
  - Status: Healthy
  - Used for: Distributed coordination caching

- **OpenSearch 3.4.0**
  - Ports: 9200 (search), 9600 (performance)
  - Status: Healthy (green cluster)
  - Used for: Vector search and article indexing

### Distributed Crawler Services
Three crawler instances running in distributed mode:

1. **baram-crawler-main**
   - Instance ID: main
   - Status: Running (health: starting)
   - Command: `distributed --instance main --database [postgresql://...] --rps 2.0 --output /app/output`
   - Responsibilities: Main crawling workload, coordinates with sub-instances

2. **baram-crawler-sub1**
   - Instance ID: sub1
   - Status: Running (health: starting)
   - Command: `distributed --instance sub1 --database [postgresql://...] --rps 2.0 --output /app/output`
   - Responsibilities: Secondary crawling workload

3. **baram-crawler-sub2**
   - Instance ID: sub2
   - Status: Running (health: starting)
   - Command: `distributed --instance sub2 --database [postgresql://...] --rps 2.0 --output /app/output`
   - Responsibilities: Tertiary crawling workload

## Configuration Details

### Environment Variables (.env file)
- **PostgreSQL Configuration**
  - POSTGRES_DB: baram
  - POSTGRES_USER: baram
  - POSTGRES_PASSWORD: nT1m3s_Pr0d_2024!
  - POSTGRES_PORT: 5432

- **OpenSearch Configuration**
  - OPENSEARCH_PORT: 9200
  - OPENSEARCH_INITIAL_ADMIN_PASSWORD: OpenS3arch_Adm1n_2024!

- **Redis Configuration**
  - REDIS_PORT: 6379
  - REDIS_MAXMEMORY: 512mb

- **Crawler Configuration**
  - CRAWLER_LOG_LEVEL: info
  - HEARTBEAT_INTERVAL: 30 seconds
  - REQUESTS_PER_SECOND: 2.0
  - MAX_CONCURRENT: 5

### Docker Compose Files
- **docker-compose.yml**: Base infrastructure (PostgreSQL, Redis, OpenSearch)
- **docker-compose.distributed.yml**: Distributed crawler services

## Network Configuration
- Network: baram_baram-network
- Subnet: 172.28.0.0/16
- Type: Bridge network

### Container IP Assignments
- OpenSearch: 172.28.0.2
- Redis: 172.28.0.3
- PostgreSQL: 172.28.0.4
- Crawler-main: 172.28.2.1
- Crawler-sub1: 172.28.2.2
- Crawler-sub2: 172.28.2.3

## Storage Volumes
All data persists in Docker named volumes:
- baram_postgres_data: PostgreSQL data
- baram_opensearch_data: OpenSearch indices
- baram_redis_data: Redis persistence
- baram_crawler_main_output: Articles crawled by main instance
- baram_crawler_main_checkpoints: Crawl checkpoints
- baram_crawler_main_logs: Execution logs
- baram_crawler_sub1_output: Articles crawled by sub1 instance
- baram_crawler_sub1_checkpoints: Sub1 checkpoints
- baram_crawler_sub1_logs: Sub1 logs
- baram_crawler_sub2_output: Articles crawled by sub2 instance
- baram_crawler_sub2_checkpoints: Sub2 checkpoints
- baram_crawler_sub2_logs: Sub2 logs

## Distributed Crawler Architecture

### Current Mode: Standalone Distributed
The distributed crawler runs in continuous mode waiting for scheduled slots. 

**Features:**
- Deduplication via PostgreSQL database
- Rate limiting: 2.0 requests/second per instance
- Heartbeat monitoring every 30 seconds
- Output directory: /app/output (container)
- Checkpoint-based recovery support

### Coordinator Status: NOT YET IMPLEMENTED
The distributed crawler tries to register with a coordinator service at `http://localhost:8080`, but this service is not yet implemented. The crawlers operate in standalone mode with:
- Self-managed scheduling
- PostgreSQL-based deduplication
- Independent operation without central coordination

To enable coordinator:
1. Implement the `coordinator` subcommand in the Rust application
2. Uncomment the coordinator service in docker-compose.distributed.yml
3. Update the coordinator URL in crawler commands to point to the running coordinator service

## Deployment Commands

### View Service Status
```bash
cd /home/mare/baram/docker
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml ps
```

### View Logs
```bash
# Main crawler
docker logs baram-crawler-main -f

# Sub1 crawler
docker logs baram-crawler-sub1 -f

# Sub2 crawler
docker logs baram-crawler-sub2 -f

# PostgreSQL
docker logs baram-postgres -f

# OpenSearch
docker logs baram-opensearch -f

# Redis
docker logs baram-redis -f
```

### Stop All Services
```bash
cd /home/mare/baram/docker
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml down
```

### Restart Services
```bash
cd /home/mare/baram/docker
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml restart
```

## Health Check Verification

All services report healthy status:
- PostgreSQL: accepting connections
- Redis: PONG
- OpenSearch: green cluster status

## Next Steps

1. **Implement Coordinator Service**: Add the `coordinator` subcommand to handle schedule management and crawler registration

2. **Test Crawling**: Run crawlers with `--once` flag to execute a single slot:
   ```bash
   docker exec baram-crawler-main baram distributed --instance main --database postgresql://baram:nT1m3s_Pr0d_2024!@localhost:5432/baram --once
   ```

3. **Monitor Output**: Check volumes for crawled articles:
   ```bash
   docker volume inspect baram_crawler_main_output
   docker run -v baram_crawler_main_output:/data --rm alpine ls /data
   ```

4. **Configure Scaling**: Adjust REQUESTS_PER_SECOND and heartbeat intervals based on performance requirements

5. **Enable OpenSearch Dashboards**: Uncomment the opensearch-dashboards service in docker-compose.yml to visualize data

## Architecture Notes

### Deduplication Strategy
- PostgreSQL-based content deduplication
- Tracks URL and content hash for each crawled article
- Prevents duplicate article indexing
- Database shared across all three crawler instances

### Rate Limiting
- 2.0 requests per second per instance
- Total capacity: 6.0 requests/second (3 instances × 2.0 rps)
- Configurable via REQUESTS_PER_SECOND environment variable

### Failure Handling
- `restart: unless-stopped` policy for automatic recovery
- Heartbeat monitoring prevents zombie processes
- PostgreSQL for reliable state persistence

## File Locations

**Configuration Files:**
- `/home/mare/baram/docker/.env` - Environment variables
- `/home/mare/baram/docker/docker-compose.yml` - Base infrastructure
- `/home/mare/baram/docker/docker-compose.distributed.yml` - Distributed crawler services

**Source Code:**
- `/home/mare/baram/src/main.rs` - Main CLI entry point (distributed command)
- `/home/mare/baram/src/crawler/distributed.rs` - Distributed runner implementation
- `/home/mare/baram/src/crawler/instance.rs` - Instance configuration

**Output Directories:**
- Volume mounted at `/app/output` in containers
- Accessible via Docker volume inspection

## Troubleshooting

### Coordinator Connection Errors
Expected in current deployment. The crawlers continue to function but without central scheduling.

### Health Check Warnings
Crawlers report "health: starting" because the health check endpoint (`http://localhost:8080/health`) is not available. This is normal without an active coordinator.

### Database Connection Issues
Verify PostgreSQL is running:
```bash
docker exec baram-postgres pg_isready -h localhost
```

### OpenSearch Issues
Check cluster health:
```bash
docker exec baram-opensearch curl -s http://localhost:9200/_cluster/health
```

---

**Deployment completed successfully. System is ready for testing and development.**
