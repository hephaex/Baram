# ktime Monitoring - Quick Start Guide

Get your monitoring stack up and running in 5 minutes.

## Prerequisites

- ktime core services running (PostgreSQL, Redis, OpenSearch)
- Coordinator service running (for distributed deployment)
- Docker and Docker Compose installed

## Step 1: Start Monitoring Stack

```bash
cd /home/mare/ktime/docker
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml up -d
```

Wait for services to start (30-60 seconds):

```bash
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml ps
```

All services should show `Up` status.

## Step 2: Access Prometheus

1. Open browser: http://localhost:9090
2. Go to Status > Targets
3. Verify all targets are healthy (green)

Expected targets:
- prometheus
- ktime-coordinator
- postgres
- redis

## Step 3: Access Grafana

1. Open browser: http://localhost:3000
2. Login with: **admin** / **admin**
3. Change password when prompted (recommended)

## Step 4: View Dashboards

Pre-configured dashboards are available:

### ktime Overview
- Click "Home" > "ktime - Distributed Crawler Overview"
- Shows real-time crawler status, articles crawled, and API performance

### Database Metrics
- Click "Home" > "ktime - Database Metrics"
- Shows PostgreSQL connections, query rates, and table activity

### Redis Metrics
- Click "Home" > "ktime - Redis Metrics"
- Shows cache performance, memory usage, and hit ratios

## Step 5: Test Metrics Collection

Run a test query in Prometheus:

1. Go to http://localhost:9090
2. Search box: type `coordinator_active_instances`
3. Click "Execute"
4. Should see current value (>= 0)

## Common Tasks

### Change Grafana Password

1. Grafana > Administration > Users > Admin
2. Click "Edit" > "Change password"
3. Save new password

### View Raw Metrics

```bash
# Coordinator metrics
curl http://localhost:8080/metrics | grep coordinator

# Database metrics
curl http://localhost:9187/metrics | grep pg_

# Redis metrics
curl http://localhost:9121/metrics | grep redis_
```

### Check Service Health

```bash
# Verify all monitoring services
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml ps

# View logs
docker-compose logs -f prometheus
docker-compose logs -f grafana

# Test metrics endpoint
curl http://localhost:9090/api/v1/targets | jq
```

### Stop Monitoring Stack

```bash
docker-compose -f docker-compose.monitoring.yml down
```

(Core services continue running)

## Troubleshooting

### Prometheus shows red X on targets

**Check if coordinator is running**:
```bash
docker-compose ps coordinator
curl http://localhost:8080/api/health
```

**Check network connectivity**:
```bash
docker-compose exec prometheus ping coordinator
```

### Grafana shows "No data"

1. Wait 60 seconds for first metrics to be collected
2. In Grafana: Configuration > Data Sources > Prometheus > Test
3. Should see green "Data source is working"

### High memory/disk usage

Check Prometheus storage:
```bash
docker-compose exec prometheus du -sh /prometheus
```

Reduce retention in `docker-compose.monitoring.yml`:
```yaml
prometheus:
  command:
    - "--storage.tsdb.retention.time=14d"  # Reduce from 30d
```

## Next Steps

- **Advanced configuration**: See `/monitoring/MONITORING.md`
- **Custom dashboards**: Export from Grafana, save to `dashboards/`
- **Alert setup**: Configure in `rules/ktime-alerts.yml`
- **Integration**: Add Slack, PagerDuty, or email alerts

## Important Metrics

### Crawler Status
- `coordinator_active_instances` - Number of online crawlers
- `coordinator_total_articles_crawled` - Total articles crawled
- `rate(coordinator_api_requests_total[5m])` - API request rate

### Database Health
- `pg_stat_activity_count` - Active database connections
- `pg_database_size_bytes` - Database size
- `rate(pg_stat_user_tables_seq_scan[5m])` - Sequential scans

### Cache Performance
- `redis_memory_used_bytes` - Redis memory usage
- `redis_keyspace_hits_total` - Cache hits
- `redis_keyspace_misses_total` - Cache misses

## Environment Variables

Add to `/docker/.env` to customize:

```bash
# Grafana credentials
GRAFANA_ADMIN_USER=admin
GRAFANA_ADMIN_PASSWORD=securepassword

# Ports
PROMETHEUS_PORT=9090
GRAFANA_PORT=3000
```

## File Structure

```
docker/monitoring/
├── prometheus.yml                 # Prometheus configuration
├── MONITORING.md                  # Full documentation
├── QUICKSTART.md                  # This file
├── rules/
│   └── ktime-alerts.yml         # Alert rules
└── grafana/
    ├── provisioning/
    │   ├── datasources/
    │   │   └── prometheus.yml     # Data source config
    │   └── dashboards/
    │       └── dashboards.yml     # Dashboard provisioning
    └── dashboards/
        ├── ktime-overview.json   # Main dashboard
        ├── database-metrics.json  # Database dashboard
        └── redis-metrics.json     # Redis dashboard
```

## Quick Dashboard Access

Direct links (after login):

- Prometheus: http://localhost:9090
- Grafana: http://localhost:3000/d/ktime-overview
- Database Dashboard: http://localhost:3000/d/ktime-database
- Redis Dashboard: http://localhost:3000/d/ktime-redis

## Performance Expectations

- **Metric collection**: Every 15 seconds
- **Metrics retention**: 30 days
- **Dashboard refresh**: 10 seconds (configurable)
- **Data storage**: ~100-200MB per day (typical setup)

## Support

For detailed information, see:
- `/monitoring/MONITORING.md` - Complete reference
- Docker logs: `docker-compose logs [service]`
- Prometheus docs: https://prometheus.io/docs/
- Grafana docs: https://grafana.com/docs/
