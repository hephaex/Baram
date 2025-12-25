# ktime Monitoring - Complete Setup Index

This document indexes all monitoring-related files and provides a roadmap for using the ktime monitoring stack.

## Overview

The ktime monitoring stack provides comprehensive observability for the distributed crawler system using:
- **Prometheus**: Metrics collection and time-series storage
- **Grafana**: Visualization and dashboards
- **Exporters**: PostgreSQL and Redis metrics

## Documentation Files

### Quick Start (5 minutes)
- **File**: `QUICKSTART.md`
- **Purpose**: Get monitoring running in 5 minutes
- **Contains**: Step-by-step instructions, common tasks, basic troubleshooting
- **Best for**: First-time setup, quick reference

### Complete Reference (comprehensive)
- **File**: `MONITORING.md`
- **Purpose**: Complete technical reference
- **Contains**: Architecture, configuration details, advanced customization, production setup
- **Best for**: Configuration changes, advanced troubleshooting, customization

### This File
- **File**: `INDEX.md`
- **Purpose**: Navigation and file reference
- **Contains**: File locations, descriptions, usage guide

## Configuration Files

### Core Configuration

```
prometheus.yml
├─ Defines what metrics to collect
├─ Sets scrape intervals (15s)
├─ Configures scrape targets:
│  ├─ coordinator:8080/metrics
│  ├─ postgres_exporter:9187
│  ├─ redis_exporter:9121
│  └─ prometheus:9090 (self)
└─ Points to alert rules
```

### Alert Configuration

```
rules/ktime-alerts.yml
├─ Coordinator alerts (3):
│  ├─ NoActiveCrawlers (CRITICAL)
│  ├─ HighCoordinatorErrorRate (WARNING)
│  └─ LowCrawlThroughput (WARNING)
├─ Database alerts (2):
│  ├─ PostgreSQLDown (CRITICAL)
│  └─ HighDatabaseConnections (WARNING)
└─ Redis alerts (2):
   ├─ RedisDown (CRITICAL)
   └─ HighRedisMemoryUsage (WARNING)
```

### Grafana Provisioning

```
grafana/provisioning/datasources/prometheus.yml
└─ Auto-configures Prometheus as data source

grafana/provisioning/dashboards/dashboards.yml
└─ Auto-loads all dashboards from:
   └─ grafana/dashboards/
```

## Dashboard Files

### 1. ktime-overview.json
**Main monitoring dashboard for crawlers**

Location: `grafana/dashboards/ktime-overview.json`

Displays:
- Online crawler instances (pie chart)
- Total articles crawled (gauge)
- API request rate (5m average)
- Crawler status per instance
- Active instances trend
- Crawl throughput

Useful for: Monitoring crawler health and performance

### 2. database-metrics.json
**Database performance and health monitoring**

Location: `grafana/dashboards/database-metrics.json`

Displays:
- Database health indicator
- Active connections
- Table scan operations
- DML operations rate
- Database size growth

Useful for: Monitoring PostgreSQL performance and capacity

### 3. redis-metrics.json
**Cache and queue performance monitoring**

Location: `grafana/dashboards/redis-metrics.json`

Displays:
- Memory usage
- Total keys
- Commands processed rate
- Cache hit/miss ratio
- Network I/O

Useful for: Monitoring Redis performance and cache efficiency

## Docker Compose Files

### docker-compose.monitoring.yml
**Main monitoring stack definition**

Location: `/home/mare/ktime/docker/docker-compose.monitoring.yml`

Services defined:
1. **prometheus**: Metrics collection (port 9090)
2. **grafana**: Dashboards and visualization (port 3000)
3. **postgres_exporter**: PostgreSQL metrics (port 9187)
4. **redis_exporter**: Redis metrics (port 9121)

Volumes:
- prometheus_data: Time-series metrics storage
- grafana_data: Dashboards and settings

Network: ktime-network

## Quick Start Guide

### 1. Start the Stack

```bash
cd /home/mare/ktime/docker
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml up -d
```

Expected output: 4 new services running

### 2. Verify Services

```bash
# Check all services are healthy
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml ps

# All should show "Up" status
```

### 3. Access Interfaces

- **Prometheus**: http://localhost:9090
  - Check targets: Status > Targets
  - Query metrics: Graph tab

- **Grafana**: http://localhost:3000
  - Login: admin/admin
  - Change password when prompted
  - Select dashboard from home

### 4. Verify Metrics Collection

```bash
# Check Prometheus targets
curl http://localhost:9090/api/v1/targets | jq

# All 4 targets should be "UP"
```

## Common Tasks

### View Raw Metrics

```bash
# Coordinator metrics
curl http://localhost:8080/metrics

# PostgreSQL metrics
curl http://localhost:9187/metrics

# Redis metrics
curl http://localhost:9121/metrics
```

### Query Specific Metrics in Prometheus

Visit http://localhost:9090/graph and enter:

- `coordinator_active_instances` - Current active crawlers
- `coordinator_total_articles_crawled` - Total crawled count
- `rate(coordinator_api_requests_total[5m])` - API request rate
- `pg_stat_activity_count` - Database connections
- `redis_memory_used_bytes` - Redis memory

### Change Grafana Password

1. Login with admin/admin
2. Click profile icon (top right)
3. Select "Change password"
4. Enter new password
5. Save

### Add Custom Dashboard

1. Create JSON file in `grafana/dashboards/`
2. Restart Grafana: `docker-compose -f docker-compose.monitoring.yml restart grafana`
3. Dashboard appears in Grafana home

### Modify Alert Rules

1. Edit `rules/ktime-alerts.yml`
2. Reload Prometheus: `curl -X POST http://localhost:9090/-/reload`
3. View alerts: http://localhost:9090/alerts

## Troubleshooting Guide

### Issue: "No data" in dashboards

**Solution**:
1. Wait 60 seconds for first metrics collection
2. Check Prometheus targets: http://localhost:9090/targets
3. Verify all are "UP" (green)
4. In Grafana: Configuration > Data Sources > Test Prometheus

### Issue: Prometheus can't reach coordinator

**Solution**:
```bash
# Verify coordinator is running
docker-compose ps coordinator

# Check from Prometheus container
docker-compose exec prometheus curl http://coordinator:8080/metrics

# Verify network
docker network inspect ktime-network
```

### Issue: High memory/disk usage

**Check storage**:
```bash
docker-compose exec prometheus du -sh /prometheus
```

**Reduce retention** in `docker-compose.monitoring.yml`:
```yaml
prometheus:
  command:
    - "--storage.tsdb.retention.time=14d"  # Reduce from 30d
```

See full troubleshooting in `MONITORING.md`

## Performance Metrics

Typical resource usage:
- Prometheus: 200-500MB RAM, 100-200MB disk/day
- Grafana: 100-200MB RAM
- Exporters: 50-100MB RAM each

Data retention: 30 days by default

Scrape interval: 15 seconds

## Production Deployment

See `MONITORING.md` for:
- Security hardening
- High-availability setup
- Long-term storage
- Alert integration (Slack, email, etc.)
- Performance tuning

## File Locations Reference

```
/home/mare/ktime/docker/
├── docker-compose.monitoring.yml          ← Main compose file
├── README.md                              ← Updated with monitoring section
└── monitoring/
    ├── INDEX.md                           ← This file
    ├── QUICKSTART.md                      ← Quick start (5 min)
    ├── MONITORING.md                      ← Complete reference
    ├── prometheus.yml                     ← Prometheus config
    ├── rules/
    │   └── ktime-alerts.yml             ← Alert rules
    └── grafana/
        ├── provisioning/
        │   ├── datasources/
        │   │   └── prometheus.yml         ← Data source config
        │   └── dashboards/
        │       └── dashboards.yml         ← Dashboard provisioning
        └── dashboards/
            ├── ktime-overview.json       ← Crawler dashboard
            ├── database-metrics.json      ← Database dashboard
            └── redis-metrics.json         ← Redis dashboard
```

## Next Steps

1. Start the monitoring stack (see Quick Start above)
2. Access Grafana at http://localhost:3000
3. Change admin password
4. Explore the 3 pre-configured dashboards
5. Review alert configuration in `rules/ktime-alerts.yml`
6. Customize dashboards and alerts as needed
7. For production: See `MONITORING.md` for hardening and integration

## Support Resources

- **Quick Questions**: See `QUICKSTART.md`
- **Configuration Help**: See `MONITORING.md`
- **PromQL Queries**: See `MONITORING.md` - Common Queries section
- **Grafana Docs**: https://grafana.com/docs/
- **Prometheus Docs**: https://prometheus.io/docs/

## Integration Examples

The monitoring stack is configured to work with:
- ktime Coordinator service (metrics endpoint)
- PostgreSQL 18 (via postgres_exporter)
- Redis 7 (via redis_exporter)
- Prometheus time-series database
- Grafana for visualization

All components are on the `ktime-network` Docker network.

## Maintenance

Regular tasks:
- Monitor storage usage (30-day retention by default)
- Review alert configuration periodically
- Check dashboard accuracy
- Backup Prometheus data (optional)

See `MONITORING.md` for backup/restore procedures.

## Customization

This setup is designed to be customizable:
- Add dashboards by dropping JSON files in `grafana/dashboards/`
- Add metrics to scrape in `prometheus.yml`
- Add alerts by editing `rules/ktime-alerts.yml`
- Adjust retention in `docker-compose.monitoring.yml`

All changes are hot-reloadable without service restart (except Prometheus restart for config changes).

---

For detailed information, refer to:
- `QUICKSTART.md` - Fast setup guide
- `MONITORING.md` - Complete technical reference
- Individual configuration files for specific settings
