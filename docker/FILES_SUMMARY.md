# Docker Files Summary

This document provides an overview of all Docker-related files created for the nTimes project.

## File Structure

```
nTimes/
├── docker/
│   ├── docker-compose.yml          # Main orchestration file
│   ├── init.sql                    # PostgreSQL schema initialization
│   ├── .env.example                # Environment variables template
│   ├── setup.sh                    # Automated setup script
│   ├── README.md                   # Docker usage guide
│   ├── opensearch/
│   │   └── opensearch.yml          # OpenSearch configuration
│   └── opensearch-index-template.json  # Index mapping for Korean text
├── Dockerfile                      # Multi-stage Rust app container
├── .dockerignore                   # Build context exclusions
├── Makefile                        # Automation commands
├── config.toml.example             # Application configuration template
└── DOCKER_SETUP.md                 # Comprehensive setup guide
```

## File Descriptions

### docker/docker-compose.yml (471 lines)
**Purpose**: Orchestrates all services with production-ready configurations

**Services**:
- PostgreSQL 18 with optimized parameters
- OpenSearch 2.11+ with Nori plugin for Korean
- Redis 7 with AOF persistence
- pgAdmin (development profile)
- OpenSearch Dashboards (development profile)

**Features**:
- Health checks for all services
- Named volumes for data persistence
- Custom network (172.28.0.0/16)
- Environment variable injection
- Resource limits and optimizations

### docker/init.sql (500+ lines)
**Purpose**: Initialize PostgreSQL database schema

**Tables Created**:
- `articles_raw` - Main article storage with full-text search
- `comments_raw` - Hierarchical comment structure
- `ontology_triples` - RDF-like knowledge graph
- `crawl_jobs` - Distributed job tracking
- `embedding_metadata` - Vector embedding references

**Features**:
- UUID support (uuid-ossp extension)
- Full-text search (pg_trgm extension)
- Automatic triggers for search vectors
- Comprehensive indexes (B-tree, GIN, composite)
- Enum types for categories and status
- Utility functions and views
- Deduplication constraints

### docker/.env.example (280 lines)
**Purpose**: Template for environment variables

**Sections**:
- PostgreSQL connection and pooling
- OpenSearch authentication and settings
- Redis configuration
- Crawler parameters
- Storage paths
- Embedding model settings
- LLM configuration (OpenAI, gemini, Ollama)
- Logging configuration
- Feature flags
- Performance tuning
- Security settings

### docker/opensearch/opensearch.yml (200+ lines)
**Purpose**: OpenSearch server configuration

**Key Settings**:
- Single-node development setup
- Korean Nori plugin enabled
- k-NN vector search support
- Memory and performance tuning
- Security configuration
- Logging levels
- Snapshot repository

### docker/opensearch-index-template.json (90 lines)
**Purpose**: Index mapping for Korean text analysis

**Features**:
- Nori tokenizer with mixed decompound mode
- Part-of-speech filtering
- k-NN vector field (1024 dimensions)
- HNSW algorithm for similarity search
- Multi-field mappings (keyword + text)

### Dockerfile (120 lines)
**Purpose**: Multi-stage build for Rust application

**Stages**:
1. Builder: Compile with optimizations
2. Runtime: Minimal Debian slim image

**Features**:
- Layer caching for dependencies
- Non-root user (UID 1001)
- Health check endpoint
- Stripped binary for smaller size
- Security best practices

### .dockerignore (250 lines)
**Purpose**: Exclude unnecessary files from build context

**Excludes**:
- Build artifacts (target/)
- IDE files (.vscode/, .idea/)
- Version control (.git/)
- Output data (output/, checkpoints/)
- Secrets and credentials
- Test files and coverage
- Documentation (except README)

### Makefile (500+ lines)
**Purpose**: Automation and developer experience

**Commands**:
- `make setup` - Initial environment setup
- `make start` - Start core services
- `make dev-tools` - Start development tools
- `make db-shell` - PostgreSQL shell
- `make opensearch-shell` - OpenSearch health check
- `make backup` - Backup all data
- `make restore` - Restore from backup
- `make build` - Build Rust app
- `make test` - Run tests
- `make clean-all` - Clean everything

### config.toml.example (150 lines)
**Purpose**: Application configuration template

**Sections**:
- Crawler settings (rate limiting, retries)
- Database connections
- OpenSearch configuration
- Storage paths
- Embedding model
- LLM providers
- Logging
- Feature flags

### docker/setup.sh (280 lines)
**Purpose**: Automated setup script

**Features**:
- Prerequisite checking
- System requirement validation (vm.max_map_count)
- Secure password generation
- Environment file creation
- Directory structure setup
- Service startup and health checks
- OpenSearch index creation
- Status display and next steps

### DOCKER_SETUP.md (800+ lines)
**Purpose**: Comprehensive setup and usage guide

**Contents**:
- Prerequisites and system requirements
- Quick start (5 minutes)
- Architecture overview
- Service details and configuration
- Common operations (backup, restore, reset)
- Integration with Rust application
- Troubleshooting guide
- Performance optimization
- Security checklist
- Monitoring and metrics
- Advanced configurations

### docker/README.md (400+ lines)
**Purpose**: Docker environment usage reference

**Contents**:
- Quick start guide
- Service overview
- Database schema description
- OpenSearch index setup
- Volume management
- Maintenance commands
- Performance tuning
- Security checklist
- Troubleshooting

## Key Features

### Production-Ready
- Health checks for all services
- Automatic restart policies
- Resource limits
- Security best practices (non-root users)
- Comprehensive error handling

### Developer-Friendly
- One-command setup (`make dev`)
- Automated setup script
- Development tools (pgAdmin, Dashboards)
- Color-coded CLI output
- Detailed documentation

### Optimized Performance
- PostgreSQL tuned for read-heavy workloads
- OpenSearch configured for Korean text
- Redis with LRU eviction
- Connection pooling
- Proper indexing strategies

### Scalability
- Horizontally scalable architecture
- Volume-based persistence
- Distributed crawling support (Redis)
- Separation of concerns

### Security
- No default passwords
- Secure password generation
- Non-root containers
- Environment-based secrets
- Network isolation

## Usage Examples

### Initial Setup
```bash
# Automated
./docker/setup.sh

# Manual
make setup
make start
make opensearch-create-index
```

### Development Workflow
```bash
make dev              # Start everything
make db-shell         # Access PostgreSQL
make logs-opensearch  # View logs
make backup           # Backup data
```

### Production Deployment
```bash
# 1. Edit docker/.env with production passwords
# 2. Update docker-compose.yml resource limits
# 3. Enable SSL/TLS
# 4. Configure monitoring
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

## Environment Variables

### Critical Variables (Must Change)
- `POSTGRES_PASSWORD` - PostgreSQL password
- `OPENSEARCH_INITIAL_ADMIN_PASSWORD` - OpenSearch admin password

### Optional Variables
- `POSTGRES_PORT` - Default: 5432
- `OPENSEARCH_PORT` - Default: 9200
- `REDIS_PORT` - Default: 6379
- `OPENSEARCH_JAVA_OPTS` - Heap size
- `REDIS_MAXMEMORY` - Cache size

## Data Persistence

All data is stored in Docker volumes:
- `postgres_data` - PostgreSQL database
- `opensearch_data` - OpenSearch indices
- `redis_data` - Redis AOF file
- `pgadmin_data` - pgAdmin configuration

## Maintenance

### Backup Strategy
```bash
# Daily backups
make backup

# Weekly full backups
docker-compose exec postgres pg_dump -U ntimes -F c ntimes > backup_weekly.dump

# OpenSearch snapshots
curl -X PUT "localhost:9200/_snapshot/backup/weekly"
```

### Monitoring
- PostgreSQL: `pg_stat_*` views
- OpenSearch: `_nodes/stats`, `_cluster/health`
- Redis: `INFO` command
- Docker: `docker stats`

## Troubleshooting Quick Reference

| Issue | Solution |
|-------|----------|
| Port in use | Change port in .env |
| Out of memory | Increase Docker memory limit |
| vm.max_map_count too low | `sudo sysctl -w vm.max_map_count=262144` |
| Password authentication failed | Check .env and config.toml match |
| OpenSearch red | Check disk space and logs |
| Connection refused | Verify service is healthy |

## Next Steps After Setup

1. Build Rust application: `cargo build --release`
2. Run test crawl: `cargo run -- crawl --category politics --max-articles 10`
3. Verify database: `make db-shell` → `SELECT COUNT(*) FROM articles_raw;`
4. Check OpenSearch: `make opensearch-indices`
5. Set up monitoring (Prometheus + Grafana)
6. Configure automated backups
7. Review security checklist

## License

GPL v3 - Copyright (c) 2024 hephaex@gmail.com
