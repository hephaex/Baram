# Docker Setup Guide for baram

This guide will help you set up the complete Docker environment for the baram Naver News Crawler.

## Prerequisites

- Docker 24.0+ ([Install Docker](https://docs.docker.com/get-docker/))
- Docker Compose ([Install Docker Compose](https://docs.docker.com/compose/install/))
- At least 4GB RAM available for Docker
- 10GB free disk space

### System Requirements by OS

#### Linux

```bash
# Increase vm.max_map_count for OpenSearch
sudo sysctl -w vm.max_map_count=262144

# Make it permanent
echo "vm.max_map_count=262144" | sudo tee -a /etc/sysctl.conf
```

#### macOS

```bash
# Increase Docker Desktop memory to at least 4GB
# Docker Desktop → Preferences → Resources → Memory
```

#### Windows (WSL2)

```powershell
# In PowerShell (Admin)
wsl -d docker-desktop
sysctl -w vm.max_map_count=262144
```

## Quick Start (5 minutes)

### Step 1: Clone and Setup

```bash
# Clone repository
git clone https://github.com/hephaex/baram.git
cd baram

# Run initial setup
make setup
```

This will:
- Copy `docker/.env.example` to `docker/.env`
- Copy `config.toml.example` to `config.toml`
- Create output directories

### Step 2: Configure Environment

Edit `docker/.env` and set secure passwords:

```bash
# Edit with your favorite editor
nano docker/.env  # or vim, code, etc.
```

**IMPORTANT**: Change these values:

```env
POSTGRES_PASSWORD=your_very_secure_password_here
OPENSEARCH_INITIAL_ADMIN_PASSWORD=Admin123!YourSecurePassword
```

Password requirements for OpenSearch:
- Minimum 8 characters
- At least one uppercase letter
- At least one lowercase letter
- At least one digit
- At least one special character

### Step 3: Start Services

```bash
# Start core services (PostgreSQL, OpenSearch, Redis)
make start

# Or manually:
cd docker
docker-compose up -d
```

Wait for services to be healthy (30-60 seconds):

```bash
# Check status
make status

# Or manually:
cd docker
docker-compose ps
```

All services should show "Up (healthy)".

### Step 4: Verify Installation

Test PostgreSQL:
```bash
make db-shell

# Inside PostgreSQL shell:
\dt  -- List tables
SELECT COUNT(*) FROM articles_raw;  -- Should return 0
\q   -- Exit
```

Test OpenSearch:
```bash
make opensearch-shell

# Should show cluster health
```

Test Redis:
```bash
make redis-shell

# Inside Redis:
PING  -- Should return PONG
exit
```

### Step 5: Create OpenSearch Index

```bash
make opensearch-create-index
```

This creates the `naver-news` index with Korean (Nori) analyzer.

## Development Environment

For development, start additional tools:

```bash
make dev-tools
```

This starts:
- **pgAdmin** - PostgreSQL admin interface at http://localhost:5050
- **OpenSearch Dashboards** - OpenSearch UI at http://localhost:5601

### Accessing pgAdmin

1. Open http://localhost:5050
2. Login with:
   - Email: `admin@baram.local` (or from your .env)
   - Password: `admin` (or from your .env)
3. Add server:
   - Name: baram
   - Host: `postgres` (Docker network name)
   - Port: `5432`
   - Database: `baram`
   - Username: `baram`
   - Password: from your `.env` file

### Accessing OpenSearch Dashboards

1. Open http://localhost:5601
2. Login with:
   - Username: `admin`
   - Password: from your `.env` (OPENSEARCH_INITIAL_ADMIN_PASSWORD)
3. Go to "Dev Tools" to run queries

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     Docker Network                           │
│  (baram-network: 172.28.0.0/16)                            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐ │
│  │  PostgreSQL  │    │  OpenSearch  │    │    Redis     │ │
│  │    :5432     │    │    :9200     │    │    :6379     │ │
│  │              │    │    :9600     │    │              │ │
│  └──────────────┘    └──────────────┘    └──────────────┘ │
│        │                    │                    │          │
│        │                    │                    │          │
│        ▼                    ▼                    ▼          │
│  postgres_data      opensearch_data        redis_data      │
│  (volume)           (volume)               (volume)         │
│                                                              │
│  ┌──────────────┐    ┌──────────────┐                     │
│  │   pgAdmin    │    │  Dashboards  │                     │
│  │    :5050     │    │    :5601     │                     │
│  │ (dev only)   │    │  (dev only)  │                     │
│  └──────────────┘    └──────────────┘                     │
│                                                              │
└─────────────────────────────────────────────────────────────┘
         ▲
         │ (Host: localhost)
         │
    ┌────┴─────┐
    │  Rust    │
    │  App     │
    └──────────┘
```

## Service Details

### PostgreSQL 18

**Purpose**: Primary relational database for structured data

**Schema**:
- `articles_raw` - Crawled news articles
- `comments_raw` - Article comments (hierarchical)
- `ontology_triples` - Knowledge graph (Subject-Predicate-Object)
- `crawl_jobs` - Job tracking for distributed crawling
- `embedding_metadata` - Vector embedding metadata

**Features**:
- Full-text search with tsvector
- UUID support for unique identifiers
- Trigram matching for fuzzy search
- Automatic timestamp triggers
- Comprehensive indexes for performance

**Performance Tuning** (production):
```sql
-- Recommended settings for 8GB RAM server
shared_buffers = 2GB
effective_cache_size = 6GB
work_mem = 16MB
maintenance_work_mem = 512MB
```

### OpenSearch 2.11+

**Purpose**: Vector database and full-text search with Korean support

**Features**:
- Nori tokenizer for Korean text analysis
- k-NN vector search (cosine similarity)
- Fuzzy matching and aggregations
- Real-time indexing

**Korean Analyzer**:
```json
{
  "analyzer": "nori_analyzer",
  "tokenizer": "nori_mixed",
  "filters": ["lowercase", "nori_posfilter"]
}
```

**Example Queries**:

Search by text:
```bash
curl -X GET "localhost:9200/naver-news/_search" \
  -u admin:PASSWORD \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {
      "match": {
        "content": "반도체 투자"
      }
    }
  }'
```

Vector similarity search:
```bash
curl -X GET "localhost:9200/naver-news/_search" \
  -u admin:PASSWORD \
  -H 'Content-Type: application/json' \
  -d '{
    "query": {
      "knn": {
        "embedding": {
          "vector": [0.1, 0.2, ...],
          "k": 10
        }
      }
    }
  }'
```

### Redis 7

**Purpose**: Distributed crawling coordination and caching

**Use Cases**:
- Crawl job queue (distributed work)
- Deduplication cache (URL visited tracking)
- Rate limiting state
- Session/temporary data

**Configuration**:
- AOF persistence (append-only file)
- LRU eviction policy
- 256MB max memory (configurable)

**Example Usage**:
```bash
# Set crawl state
SET crawl:job:123:status "processing"
EXPIRE crawl:job:123:status 3600

# Check if URL visited
SISMEMBER crawled:urls "https://news.naver.com/..."

# Rate limiting
INCR rate:limit:naver
EXPIRE rate:limit:naver 1
```

## Common Operations

### Backup

Backup all data:
```bash
make backup
```

This creates:
- `backups/postgres_YYYYMMDD_HHMMSS.sql` - PostgreSQL dump

For OpenSearch:
```bash
# Configure snapshot repository first
curl -X PUT "localhost:9200/_snapshot/backup" \
  -u admin:PASSWORD \
  -H 'Content-Type: application/json' \
  -d '{
    "type": "fs",
    "settings": {
      "location": "/usr/share/opensearch/backup"
    }
  }'

# Create snapshot
curl -X PUT "localhost:9200/_snapshot/backup/snapshot_1" \
  -u admin:PASSWORD
```

### Restore

Restore PostgreSQL:
```bash
make restore FILE=backups/postgres_20240115_120000.sql
```

### Reset Database

**WARNING**: This deletes all data!

```bash
make db-reset
```

### View Logs

```bash
# All services
make logs

# Specific service
make logs-postgres
make logs-opensearch
make logs-redis
```

### Clean Everything

**WARNING**: This removes all volumes and data!

```bash
make clean-all
```

## Integration with Rust Application

### Update Configuration

Edit `config.toml`:

```toml
[postgresql]
host = "localhost"
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

### Environment Variables (Recommended)

Create `.env` in project root:

```bash
DATABASE_URL=postgresql://baram:password@localhost:5432/baram
OPENSEARCH_URL=http://localhost:9200
OPENSEARCH_PASSWORD=your_password
REDIS_URL=redis://localhost:6379
```

Load in Rust:
```rust
use std::env;

let db_url = env::var("DATABASE_URL")?;
let opensearch_url = env::var("OPENSEARCH_URL")?;
```

### Running the Application

```bash
# Build
cargo build --release

# Run crawler
cargo run --release -- crawl --category politics --max-articles 100

# Run search
cargo run --release -- search "반도체 투자"

# Run with environment variables
source .env && cargo run --release -- crawl
```

## Troubleshooting

### OpenSearch: "max virtual memory areas too low"

```bash
# Linux
sudo sysctl -w vm.max_map_count=262144

# Docker Desktop (Mac/Windows)
# Increase memory in Docker Desktop settings
```

### PostgreSQL: "password authentication failed"

- Check `.env` file has correct password
- Ensure `POSTGRES_PASSWORD` matches in both `docker/.env` and `config.toml`
- Try resetting: `docker-compose down -v && docker-compose up -d`

### OpenSearch: "cluster health is red"

- Check logs: `make logs-opensearch`
- Increase heap size in `docker-compose.yml`:
  ```yaml
  OPENSEARCH_JAVA_OPTS=-Xms1g -Xmx1g
  ```
- Check disk space

### Redis: "Connection refused"

- Verify Redis is running: `docker-compose ps redis`
- Check port is not in use: `lsof -i :6379`
- Try restart: `docker-compose restart redis`

### Port Already in Use

Change ports in `docker/.env`:
```env
POSTGRES_PORT=5433
OPENSEARCH_PORT=9201
REDIS_PORT=6380
```

### Out of Disk Space

```bash
# Check Docker disk usage
docker system df

# Clean up unused resources
docker system prune -a --volumes
```

## Performance Optimization

### PostgreSQL

For production with 16GB RAM:

```yaml
# In docker-compose.yml
command:
  - "postgres"
  - "-c"
  - "shared_buffers=4GB"
  - "-c"
  - "effective_cache_size=12GB"
  - "-c"
  - "work_mem=32MB"
  - "-c"
  - "maintenance_work_mem=1GB"
```

### OpenSearch

For production with 16GB RAM:

```yaml
environment:
  - "OPENSEARCH_JAVA_OPTS=-Xms8g -Xmx8g"
```

Rules:
- Heap size: 50% of RAM (max 32GB)
- Leave 50% for OS page cache
- Use same value for Xms and Xmx

### Redis

For high-throughput caching:

```yaml
command:
  - "--maxmemory 2gb"
  - "--maxmemory-policy allkeys-lru"
  - "--appendonly yes"
```

## Security Checklist

Before deploying to production:

- [ ] Change all default passwords
- [ ] Enable SSL/TLS for PostgreSQL
- [ ] Enable SSL/TLS for OpenSearch
- [ ] Enable OpenSearch security plugin
- [ ] Configure firewall rules
- [ ] Use secrets management (HashiCorp Vault, AWS Secrets Manager)
- [ ] Enable audit logging
- [ ] Set up monitoring and alerting
- [ ] Configure automated backups
- [ ] Review container security (scan for vulnerabilities)
- [ ] Use non-root users (already configured)
- [ ] Implement network policies
- [ ] Enable log rotation
- [ ] Set resource limits (CPU, memory)
- [ ] Use private Docker registry

## Monitoring

### Health Checks

```bash
# Check all services
docker-compose ps

# PostgreSQL
docker-compose exec postgres pg_isready -U baram

# OpenSearch
curl -u admin:PASSWORD http://localhost:9200/_cluster/health?pretty

# Redis
docker-compose exec redis redis-cli PING
```

### Metrics

PostgreSQL statistics:
```sql
SELECT * FROM pg_stat_activity;
SELECT * FROM pg_stat_database WHERE datname = 'baram';
```

OpenSearch stats:
```bash
curl -u admin:PASSWORD http://localhost:9200/_nodes/stats?pretty
curl -u admin:PASSWORD http://localhost:9200/_cluster/stats?pretty
```

Redis info:
```bash
docker-compose exec redis redis-cli INFO
```

## Advanced Configuration

### Multi-node OpenSearch Cluster

For production, use multiple nodes:

```yaml
opensearch-node-1:
  environment:
    - cluster.name=baram-cluster
    - node.name=baram-node-1
    - discovery.seed_hosts=opensearch-node-2,opensearch-node-3
    - cluster.initial_master_nodes=baram-node-1,baram-node-2,baram-node-3

opensearch-node-2:
  # Similar configuration

opensearch-node-3:
  # Similar configuration
```

### PostgreSQL Replication

Set up read replicas for scaling reads:

```yaml
postgres-replica:
  image: postgres:18-alpine
  environment:
    POSTGRES_MASTER_HOST: postgres
    POSTGRES_REPLICATION_USER: replicator
    POSTGRES_REPLICATION_PASSWORD: replica_password
```

### Redis Cluster

For high availability:

```yaml
redis-1:
  command: redis-server --cluster-enabled yes --cluster-config-file nodes.conf

redis-2:
  # Similar configuration

redis-3:
  # Similar configuration
```

## Next Steps

1. **Build the Rust application**: `cargo build --release`
2. **Run initial crawl**: `make run-crawl ARGS="--category politics --max-articles 10"`
3. **Verify data**: Check database and OpenSearch for crawled data
4. **Set up monitoring**: Implement Prometheus + Grafana
5. **Configure backups**: Set up automated backup schedule
6. **Scale services**: Add replicas for high availability
7. **Optimize queries**: Analyze slow queries and add indexes

## Support

For issues and questions:
- GitHub Issues: https://github.com/hephaex/baram/issues
- Documentation: See `docker/README.md`

## License

GPL v3 - Copyright (c) 2024 hephaex@gmail.com
