# Docker Environment for baram

This directory contains Docker configurations for running the baram Naver News Crawler infrastructure.

## Quick Start

### 1. Environment Setup

Copy the example environment file and configure it:

```bash
cp docker/.env.example docker/.env
```

Edit `docker/.env` and set secure passwords:

```bash
# Required: Change these passwords!
POSTGRES_PASSWORD=your_secure_password_here
OPENSEARCH_INITIAL_ADMIN_PASSWORD=Admin123!SecurePassword
```

### 2. Start Services

Start core services (PostgreSQL, OpenSearch, Redis):

```bash
cd docker
docker-compose up -d
```

Start with development tools (pgAdmin, OpenSearch Dashboards):

```bash
docker-compose --profile development up -d
```

### 3. Verify Services

Check that all services are healthy:

```bash
docker-compose ps
```

Test PostgreSQL connection:

```bash
docker-compose exec postgres psql -U baram -d baram -c "SELECT version();"
```

Test OpenSearch:

```bash
curl -u admin:YOUR_PASSWORD http://localhost:9200/_cluster/health?pretty
```

Test Redis:

```bash
docker-compose exec redis redis-cli ping
```

## Monitoring Stack (Prometheus & Grafana)

For comprehensive monitoring of the baram infrastructure:

### Start Monitoring Services

```bash
cd docker
# Start core services + monitoring stack
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml up -d
```

This will start:
- Prometheus (metrics collection and storage) on port 9090
- Grafana (visualization and dashboards) on port 3000
- PostgreSQL Exporter (database metrics)
- Redis Exporter (cache metrics)

### Access Monitoring UIs

**Prometheus**:
- URL: http://localhost:9090
- Purpose: Metrics querying and exploration
- Features: Graph building, alerts view, targets health check

**Grafana**:
- URL: http://localhost:3000
- Default credentials: admin / admin (change in .env)
- Pre-configured dashboards:
  - baram Distributed Crawler Overview
  - Database Metrics
  - Redis Metrics

### Configure Grafana Credentials

Set secure Grafana credentials in `.env`:

```bash
GRAFANA_ADMIN_USER=your_admin_username
GRAFANA_ADMIN_PASSWORD=your_secure_password
```

### Monitoring Architecture

The monitoring stack collects metrics from:

1. **Coordinator**: Crawler instance status, job stats, API performance
2. **PostgreSQL**: Connection count, query stats, table activity, database size
3. **Redis**: Memory usage, key counts, cache hit ratio, command rate
4. **Prometheus**: Self-monitoring and metrics storage health

Metrics are collected every 15 seconds and stored for 30 days.

### Create Custom Dashboards

Add new dashboards in `/monitoring/grafana/dashboards/`:

1. Create dashboard JSON file
2. Configure in `/monitoring/grafana/provisioning/dashboards/dashboards.yml`
3. Restart Grafana: `docker-compose -f docker-compose.monitoring.yml restart grafana`

### Alert Configuration

Alerts are defined in `/monitoring/rules/baram-alerts.yml`:

- **No Active Crawlers**: Critical when no instances are online
- **High Error Rate**: Warning when API error rate > 5%
- **Low Throughput**: Warning when crawl rate drops below threshold
- **Database Issues**: Connection or health alerts
- **Redis Issues**: Memory usage and connectivity alerts

### Metric Retention and Storage

Default configuration:
- Retention: 30 days
- Storage location: Docker volume `prometheus_data`
- Scrape interval: 15 seconds

Modify in `docker-compose.monitoring.yml`:

```yaml
prometheus:
  command:
    - "--storage.tsdb.retention.time=90d"  # Change retention
    - "--storage.tsdb.retention.size=50GB" # Limit storage size
```

### Backup Prometheus Data

```bash
# Backup
docker-compose exec prometheus tar czf /prometheus/backup.tar.gz /prometheus/

# Copy to host
docker cp baram-prometheus:/prometheus/backup.tar.gz ./prometheus-backup.tar.gz

# Restore (after replacing prometheus_data volume)
docker cp ./prometheus-backup.tar.gz baram-prometheus:/prometheus/
docker-compose exec prometheus tar xzf /prometheus/backup.tar.gz
```

### Verify Monitoring Setup

```bash
# Check all monitoring services
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml ps

# View logs
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml logs -f prometheus
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml logs -f grafana

# Test metrics collection
curl http://localhost:9090/api/v1/targets
```

### Performance Tips

1. **Reduce scrape frequency** for large deployments (increase scrape_interval)
2. **Use recording rules** for frequently used metric calculations
3. **Implement service discovery** for dynamic crawler scaling
4. **Set up persistent storage** for metrics in production

### Troubleshooting Monitoring

**Prometheus can't reach coordinator**:
```bash
# Check coordinator health
curl http://localhost:8080/api/health
# Verify network connectivity
docker-compose exec prometheus ping coordinator
```

**Grafana dashboards not loading**:
- Check Prometheus connectivity: Grafana > Configuration > Data Sources
- Verify dashboard provisioning path is mounted correctly
- Check Grafana logs: `docker-compose logs grafana`

**High memory usage**:
- Review retention policy
- Consider splitting dashboards to reduce query load
- Implement metric relabeling to drop unnecessary labels

## Distributed Crawler Deployment

For running multiple crawler instances in a distributed setup:

### Start Distributed Crawlers

```bash
cd docker
# Start core services + 3 crawler instances (main, sub1, sub2)
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml up -d
```

This will start:
- Core services (PostgreSQL, OpenSearch, Redis)
- 3 crawler instances with distributed coordination
- Each instance runs independently with database-backed coordination

### Verify Distributed Setup

```bash
# Check all services
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml ps

# View crawler logs
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml logs -f crawler-main
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml logs -f crawler-sub1
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml logs -f crawler-sub2
```

### Stop Distributed Crawlers

```bash
# Stop all services
docker-compose -f docker-compose.yml -f docker-compose.distributed.yml down

# Stop only crawler instances (keep core services running)
docker stop baram-crawler-main baram-crawler-sub1 baram-crawler-sub2
```

## Services Overview

### PostgreSQL 18

- **Port**: 5432
- **Database**: baram
- **Purpose**: Primary storage for articles, comments, and ontology triples
- **Features**: Full-text search, UUID support, trigram matching

**Access pgAdmin** (development profile):
- URL: http://localhost:5050
- Email: admin@baram.local (configurable in .env)
- Password: admin (configurable in .env)

### OpenSearch 2.11+

- **Port**: 9200 (REST API), 9600 (Performance Analyzer)
- **Purpose**: Vector database with Korean (Nori) text analysis
- **Features**: k-NN search, full-text search with Korean language support

**Access OpenSearch Dashboards** (development profile):
- URL: http://localhost:5601
- Username: admin
- Password: YOUR_OPENSEARCH_PASSWORD

### Redis 7

- **Port**: 6379
- **Purpose**: Distributed crawling coordination, caching
- **Configuration**: LRU eviction, AOF persistence

## Database Schema

The PostgreSQL database is initialized with the following tables:

- **articles_raw**: Crawled news articles with full-text search
- **comments_raw**: Article comments with hierarchical structure
- **ontology_triples**: Knowledge graph (Subject-Predicate-Object)
- **crawl_jobs**: Job tracking for distributed crawling
- **embedding_metadata**: Vector embedding metadata

See `init.sql` for complete schema definition.

## OpenSearch Index Setup

After starting OpenSearch, create the index for Korean text:

```bash
curl -X PUT "localhost:9200/naver-news" \
  -u admin:YOUR_PASSWORD \
  -H 'Content-Type: application/json' \
  -d '{
    "settings": {
      "number_of_shards": 1,
      "number_of_replicas": 0,
      "analysis": {
        "tokenizer": {
          "nori_mixed": {
            "type": "nori_tokenizer",
            "decompound_mode": "mixed"
          }
        },
        "analyzer": {
          "nori_analyzer": {
            "type": "custom",
            "tokenizer": "nori_mixed",
            "filter": ["nori_posfilter", "lowercase"]
          }
        },
        "filter": {
          "nori_posfilter": {
            "type": "nori_part_of_speech",
            "stoptags": ["E", "IC", "J", "MAG", "MM", "SP", "SSC", "SSO", "SC", "SE", "XPN", "XSA", "XSN", "XSV", "UNA", "NA", "VSV"]
          }
        }
      }
    },
    "mappings": {
      "properties": {
        "title": {
          "type": "text",
          "analyzer": "nori_analyzer"
        },
        "content": {
          "type": "text",
          "analyzer": "nori_analyzer"
        },
        "embedding": {
          "type": "knn_vector",
          "dimension": 1024,
          "method": {
            "name": "hnsw",
            "space_type": "cosinesimil",
            "engine": "nmslib"
          }
        },
        "category": {
          "type": "keyword"
        },
        "publisher": {
          "type": "keyword"
        },
        "published_at": {
          "type": "date"
        }
      }
    }
  }'
```

## Volume Management

Data is persisted in Docker volumes:

```bash
# List volumes
docker volume ls | grep baram

# Backup PostgreSQL data
docker-compose exec postgres pg_dump -U baram baram > backup.sql

# Backup OpenSearch data
docker-compose exec opensearch tar czf /tmp/opensearch-backup.tar.gz /usr/share/opensearch/data
docker cp baram-opensearch:/tmp/opensearch-backup.tar.gz ./opensearch-backup.tar.gz

# Restore PostgreSQL data
docker-compose exec -T postgres psql -U baram baram < backup.sql
```

## Maintenance

### View Logs

```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f postgres
docker-compose logs -f opensearch
docker-compose logs -f redis
```

### Restart Services

```bash
# Restart all
docker-compose restart

# Restart specific service
docker-compose restart postgres
```

### Stop Services

```bash
# Stop all (keeps data)
docker-compose down

# Stop and remove volumes (WARNING: deletes all data)
docker-compose down -v
```

## Performance Tuning

### PostgreSQL

The configuration in `docker-compose.yml` is optimized for development. For production:

1. Increase `shared_buffers` to 25% of RAM
2. Set `effective_cache_size` to 75% of RAM
3. Adjust `work_mem` based on concurrent connections
4. Enable connection pooling (e.g., PgBouncer)

### OpenSearch

For production deployments:

1. Increase heap size: `-Xms4g -Xmx4g` (adjust based on RAM)
2. Enable security plugin with SSL/TLS
3. Set `number_of_replicas` >= 1 for high availability
4. Use dedicated master nodes for clusters
5. Configure snapshot repository for backups

### Redis

For production:

1. Increase `maxmemory` based on cache needs
2. Consider RDB snapshots for persistence
3. Enable Redis Sentinel for high availability
4. Use Redis Cluster for horizontal scaling

## Security Checklist

Before deploying to production:

- [ ] Change all default passwords in `.env`
- [ ] Enable SSL/TLS for PostgreSQL
- [ ] Enable SSL/TLS for OpenSearch
- [ ] Configure firewall rules (only expose necessary ports)
- [ ] Enable OpenSearch security plugin
- [ ] Set up proper authentication and authorization
- [ ] Configure audit logging
- [ ] Review and restrict network access
- [ ] Implement secrets management (e.g., HashiCorp Vault)
- [ ] Set up automated backups
- [ ] Configure monitoring and alerting
- [ ] Review container security (scan images, use non-root users)

## Troubleshooting

### PostgreSQL Won't Start

Check logs:
```bash
docker-compose logs postgres
```

Common issues:
- Port 5432 already in use
- Insufficient disk space
- Permission issues with volumes

### OpenSearch Won't Start

Check memory lock settings:
```bash
# On Linux, increase vm.max_map_count
sudo sysctl -w vm.max_map_count=262144
```

Make it permanent:
```bash
echo "vm.max_map_count=262144" | sudo tee -a /etc/sysctl.conf
```

### Connection Refused Errors

Ensure services are healthy:
```bash
docker-compose ps
```

Wait for health checks to pass (especially OpenSearch takes 30-60s to start).

## Development Tools

### Connect to PostgreSQL

Using psql:
```bash
docker-compose exec postgres psql -U baram -d baram
```

Using pgAdmin:
1. Open http://localhost:5050
2. Add server:
   - Host: postgres (Docker network name)
   - Port: 5432
   - Database: baram
   - Username: baram
   - Password: from .env file

### Query OpenSearch

Using curl:
```bash
# Search
curl -u admin:PASSWORD "http://localhost:9200/naver-news/_search?q=반도체&pretty"

# Get index stats
curl -u admin:PASSWORD "http://localhost:9200/naver-news/_stats?pretty"
```

Using OpenSearch Dashboards:
1. Open http://localhost:5601
2. Dev Tools → Console
3. Run queries interactively

## Integration with Rust Application

Update your `config.toml`:

```toml
[postgresql]
host = "localhost"  # or "postgres" if running in Docker network
port = 5432
database = "baram"
username = "baram"
password = "your_password_from_env"

[opensearch]
hosts = ["http://localhost:9200"]
username = "admin"
password = "your_opensearch_password"

[redis]
url = "redis://localhost:6379"
```

Or use environment variables (recommended):

```bash
export DATABASE_URL="postgresql://baram:password@localhost:5432/baram"
export OPENSEARCH_URL="http://localhost:9200"
export OPENSEARCH_PASSWORD="your_password"
export REDIS_URL="redis://localhost:6379"
```

## License

GPL v3 - Copyright (c) 2024 hephaex@gmail.com
