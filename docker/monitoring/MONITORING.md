# nTimes Monitoring Stack Documentation

This document provides comprehensive information about the Prometheus and Grafana monitoring stack for the nTimes distributed crawler system.

## Overview

The monitoring stack consists of:

1. **Prometheus** - Time-series database for metrics collection and storage
2. **Grafana** - Visualization and dashboard platform
3. **PostgreSQL Exporter** - Exports database metrics
4. **Redis Exporter** - Exports cache metrics
5. **Alert Rules** - Automated alerting based on metric conditions

## Quick Start

### Start the Complete Stack

```bash
cd /home/mare/nTimes/docker
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml up -d
```

### Access the Dashboards

- **Prometheus**: http://localhost:9090
- **Grafana**: http://localhost:3000 (admin / admin by default)

## Architecture

### Metric Flow

```
Coordinator Service
      |
      v
Prometheus (Scrape every 15s)
      |
      +----> Time-Series Database (30 day retention)
      |
      v
Grafana Dashboards
      |
      +----> Pre-configured Visualizations
      |
      v
Alert Rules
      |
      +----> Alert Manager (future enhancement)
```

### Network Configuration

All services run on the `ntimes-network` bridge network:

- Coordinator: `http://coordinator:8080/metrics`
- PostgreSQL Exporter: `postgres_exporter:9187`
- Redis Exporter: `redis_exporter:9121`
- Prometheus: `prometheus:9090`
- Grafana: `grafana:3000`

## Configuration Files

### Prometheus Configuration

**Location**: `/home/mare/nTimes/docker/monitoring/prometheus.yml`

Key sections:

- **Global settings**: Scrape interval (15s), evaluation interval
- **Scrape configs**: Defines what metrics to collect from which targets
- **Alert rules**: Points to rule files for alert definitions

### Grafana Provisioning

**Datasources**: `/home/mare/nTimes/docker/monitoring/grafana/provisioning/datasources/`
- Automatically configures Prometheus as the data source
- Connection: `http://prometheus:9090`

**Dashboards**: `/home/mare/nTimes/docker/monitoring/grafana/provisioning/dashboards/`
- Automatic dashboard loading from JSON files
- Dashboard location: `/home/mare/nTimes/docker/monitoring/grafana/dashboards/`

### Alert Rules

**Location**: `/home/mare/nTimes/docker/monitoring/rules/ntimes-alerts.yml`

Rules are organized by service:
- `ntimes-coordinator`: Crawler-specific alerts
- `ntimes-database`: PostgreSQL-specific alerts
- `ntimes-redis`: Redis-specific alerts

## Included Dashboards

### 1. nTimes - Distributed Crawler Overview

**File**: `ntimes-overview.json`

Displays:
- Online crawler instances (pie chart)
- Total articles crawled (gauge)
- API request rate (time-series)
- Crawler instance status (stat)
- Active instances over time (bar chart)
- Crawl throughput (time-series)

**Key Metrics**:
- `coordinator_active_instances`
- `coordinator_total_articles_crawled`
- `coordinator_api_requests_total`
- `coordinator_crawler_status`

### 2. nTimes - Database Metrics

**File**: `database-metrics.json`

Displays:
- Database health (gauge)
- Active database connections (gauge)
- Table scan operations (time-series)
- DML operations rate (time-series)
- Database size (time-series)

**Key Metrics**:
- `pg_stat_activity_count`
- `pg_stat_user_tables_seq_scan`
- `pg_stat_user_tables_idx_scan`
- `pg_stat_user_tables_n_tup_ins/upd/del`
- `pg_database_size_bytes`

### 3. nTimes - Redis Metrics

**File**: `redis-metrics.json`

Displays:
- Memory used (gauge)
- Total keys (gauge)
- Commands processed rate (time-series)
- Cache hit/miss rate (time-series)
- Network I/O rate (time-series)

**Key Metrics**:
- `redis_memory_used_bytes`
- `redis_db_keys`
- `redis_commands_processed_total`
- `redis_keyspace_hits_total`
- `redis_keyspace_misses_total`
- `redis_net_input_bytes_total`
- `redis_net_output_bytes_total`

## Alerts

### Coordinator Alerts

#### NoActiveCrawlers (CRITICAL)

- **Condition**: No crawler instances registered
- **Duration**: Triggers after 2 minutes
- **Action**: Check coordinator health and instance registration

#### HighCoordinatorErrorRate (WARNING)

- **Condition**: API error rate > 5%
- **Duration**: Triggers after 5 minutes
- **Action**: Review coordinator logs and API endpoints

#### LowCrawlThroughput (WARNING)

- **Condition**: Crawl rate < 0.1 articles/second
- **Duration**: Triggers after 10 minutes
- **Action**: Check crawler health and queue status

### Database Alerts

#### PostgreSQLDown (CRITICAL)

- **Condition**: Exporter cannot connect to database
- **Duration**: Triggers after 1 minute
- **Action**: Check database service health

#### HighDatabaseConnections (WARNING)

- **Condition**: > 150 active connections
- **Duration**: Triggers after 5 minutes
- **Action**: Review long-running queries and connection pools

### Redis Alerts

#### RedisDown (CRITICAL)

- **Condition**: Exporter cannot connect to Redis
- **Duration**: Triggers after 1 minute
- **Action**: Check Redis service health

#### HighRedisMemoryUsage (WARNING)

- **Condition**: Memory usage > 85% of max
- **Duration**: Triggers after 5 minutes
- **Action**: Review cached data and eviction policies

## Prometheus Queries

### Common Queries for Monitoring

**Crawler Status**:
```promql
coordinator_active_instances
```

**Crawl Rate** (articles per minute):
```promql
rate(coordinator_total_articles_crawled[1m]) * 60
```

**API Request Rate** (requests per second):
```promql
rate(coordinator_api_requests_total[5m])
```

**Database Connections**:
```promql
pg_stat_activity_count
```

**Redis Memory Usage %**:
```promql
redis_memory_used_bytes / redis_memory_max_bytes * 100
```

**Cache Hit Ratio**:
```promql
rate(redis_keyspace_hits_total[5m]) / (rate(redis_keyspace_hits_total[5m]) + rate(redis_keyspace_misses_total[5m]))
```

## Customization

### Adding a New Dashboard

1. Create a new JSON file in `/monitoring/grafana/dashboards/`
   ```bash
   touch /home/mare/nTimes/docker/monitoring/grafana/dashboards/my-dashboard.json
   ```

2. Add dashboard JSON content (can export from Grafana UI)

3. Update provisioning config:
   ```yaml
   # /monitoring/grafana/provisioning/dashboards/dashboards.yml
   providers:
     - name: 'nTimes Dashboards'
       path: /var/lib/grafana/dashboards
   ```

4. Restart Grafana:
   ```bash
   docker-compose -f docker-compose.monitoring.yml restart grafana
   ```

### Adding New Alert Rules

1. Create or update rule files in `/monitoring/rules/`

2. Rules must be in YAML format:
   ```yaml
   groups:
     - name: my-alerts
       rules:
         - alert: MyAlert
           expr: metric > threshold
           for: 5m
   ```

3. Reload Prometheus:
   ```bash
   curl -X POST http://localhost:9090/-/reload
   ```

### Modifying Scrape Configuration

Edit `/monitoring/prometheus.yml` to:
- Add new targets
- Change scrape intervals
- Modify labels

Changes require Prometheus restart:
```bash
docker-compose -f docker-compose.monitoring.yml restart prometheus
```

## Performance Tuning

### Reduce Metric Storage

**Increase retention time**:
```yaml
prometheus:
  command:
    - "--storage.tsdb.retention.time=90d"  # Default 30d
```

**Limit storage size**:
```yaml
prometheus:
  command:
    - "--storage.tsdb.retention.size=50GB"  # Stop accepting metrics
```

### Optimize Prometheus

**Scrape less frequently** for reduced overhead:
```yaml
global:
  scrape_interval: 30s  # Default 15s
```

**Use recording rules** for repeated calculations:
```yaml
rule_files:
  - '/etc/prometheus/recording_rules.yml'
```

### Optimize Grafana

- Reduce dashboard refresh frequency
- Limit number of panels per dashboard
- Use `range queries` instead of instant queries
- Implement metric relabeling to drop unnecessary labels

## Maintenance

### Backup Prometheus Data

```bash
# Create backup
docker-compose exec prometheus tar czf /prometheus/backup.tar.gz -C / prometheus

# Copy to host
docker cp ntimes-prometheus:/prometheus/backup.tar.gz ./prometheus-backup-$(date +%Y%m%d).tar.gz
```

### Restore Prometheus Data

```bash
# Copy backup to container
docker cp ./prometheus-backup-20240101.tar.gz ntimes-prometheus:/prometheus/

# Extract (with Prometheus stopped)
docker-compose -f docker-compose.monitoring.yml stop prometheus
docker-compose exec prometheus tar xzf /prometheus/backup.tar.gz
docker-compose -f docker-compose.monitoring.yml start prometheus
```

### Rotate Metrics Data

Prometheus automatically purges data older than retention period. No manual rotation needed.

### Monitor Monitoring Stack

Check Prometheus targets:
```bash
curl http://localhost:9090/api/v1/targets | jq
```

View Prometheus WAL (write-ahead log):
```bash
docker-compose exec prometheus ls -lh /prometheus/wal/
```

Check storage usage:
```bash
docker-compose exec prometheus du -sh /prometheus/
```

## Troubleshooting

### Prometheus Can't Reach Coordinator

**Symptom**: Red X in Prometheus targets list

**Check**:
```bash
# Test coordinator endpoint
curl http://localhost:8080/api/health

# Test from Prometheus container
docker-compose exec prometheus curl http://coordinator:8080/metrics
```

**Fix**:
- Ensure coordinator is running: `docker-compose ps coordinator`
- Check network: `docker network ls | grep ntimes`
- Verify coordinator metrics endpoint is exposed

### Grafana Dashboards Not Loading

**Symptom**: Panels show "No data"

**Check**:
```bash
# Verify Prometheus connectivity
# In Grafana: Configuration > Data Sources > Prometheus > Save & Test

# Check dashboard provisioning
docker-compose logs grafana | grep -i provisioning
```

**Fix**:
- Ensure Prometheus is healthy and has collected metrics
- Verify data source URL is correct
- Wait for metrics to be collected (first scrape after 15 seconds)
- Check Grafana logs for provisioning errors

### High Memory Usage

**Symptom**: Prometheus or Grafana consuming excessive memory

**Check**:
```bash
# Monitor Prometheus memory
docker stats ntimes-prometheus

# Check storage size
docker-compose exec prometheus du -sh /prometheus/
```

**Fix**:
- Reduce metric retention time
- Decrease scrape frequency
- Drop unnecessary metrics/labels using relabeling
- Split large dashboards
- Increase container memory limits

### Missing Metrics

**Symptom**: Some panels show "No data"

**Check**:
1. Verify target is reachable in Prometheus UI
2. Check if metric exists: http://localhost:9090/graph
3. Review scrape configuration in `prometheus.yml`

**Fix**:
- Ensure exporters are running (postgres_exporter, redis_exporter)
- Verify container networking
- Check exporter logs for errors
- Reload Prometheus configuration

### Disk Space Issues

**Symptom**: Prometheus stops accepting metrics

**Check**:
```bash
docker-compose exec prometheus df -h /prometheus
```

**Fix**:
- Reduce retention time
- Implement retention by size limit
- Move Docker volumes to larger filesystem
- Clean up old backups

## Security Considerations

### Production Recommendations

1. **Change default Grafana credentials**:
   ```bash
   GRAFANA_ADMIN_USER=monitoring
   GRAFANA_ADMIN_PASSWORD=secure_password_here
   ```

2. **Restrict network access**:
   - Use reverse proxy (nginx) for Grafana
   - Implement authentication/authorization
   - Use firewall rules

3. **Secure Prometheus**:
   - Only expose to trusted networks
   - No authentication by default (use reverse proxy)
   - Consider read-only mode for dashboards

4. **Data sensitivity**:
   - Monitor metrics for sensitive information
   - Implement data retention policies
   - Backup encryption

## Integration with Existing Systems

### Slack Alerting (Future)

To integrate with Slack, configure Alertmanager:

1. Create webhook to Slack
2. Update AlertManager configuration
3. Point Prometheus alerts to AlertManager

### External Storage (Future)

For long-term storage beyond 30 days:

- Configure remote storage backends
- Use cloud storage (S3, GCS, Azure)
- Implement data warehouse integration

## Additional Resources

- [Prometheus Documentation](https://prometheus.io/docs/)
- [Grafana Documentation](https://grafana.com/docs/)
- [Prometheus Exporters](https://prometheus.io/docs/instrumenting/exporters/)
- [PromQL Query Guide](https://prometheus.io/docs/prometheus/latest/querying/basics/)

## Support and Issues

For monitoring-related issues:

1. Check this documentation first
2. Review container logs: `docker-compose logs <service>`
3. Verify network connectivity between containers
4. Check Prometheus targets page: http://localhost:9090/targets
5. Review Grafana logs and data source configuration
