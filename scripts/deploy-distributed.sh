#!/bin/bash
# deploy-distributed.sh - Deploy distributed crawler system
#
# Usage:
#   ./scripts/deploy-distributed.sh [command]
#
# Commands:
#   start       - Start all services (coordinator + 3 crawlers)
#   stop        - Stop all services
#   restart     - Restart all services
#   status      - Show service status
#   logs        - Show logs (follow mode)
#   build       - Build Docker images
#   scale       - Scale crawler instances
#   health      - Check health of all services
#   clean       - Remove all containers and volumes
#
# Environment variables:
#   COMPOSE_PROJECT_NAME - Project name (default: ntimes)
#   ENV_FILE            - Environment file path (default: docker/.env)

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DOCKER_DIR="$PROJECT_ROOT/docker"

COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-ntimes}"
ENV_FILE="${ENV_FILE:-$DOCKER_DIR/.env}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."

    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed"
        exit 1
    fi

    if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
        log_error "Docker Compose is not installed"
        exit 1
    fi

    if [ ! -f "$ENV_FILE" ]; then
        log_warn "Environment file not found: $ENV_FILE"
        log_info "Creating from example..."
        if [ -f "$DOCKER_DIR/.env.example" ]; then
            cp "$DOCKER_DIR/.env.example" "$ENV_FILE"
            log_warn "Please edit $ENV_FILE with your settings"
        else
            log_error "No .env.example found. Please create $ENV_FILE manually"
            exit 1
        fi
    fi

    log_success "Prerequisites check passed"
}

# Get docker compose command (v1 or v2)
get_compose_cmd() {
    if docker compose version &> /dev/null 2>&1; then
        echo "docker compose"
    else
        echo "docker-compose"
    fi
}

# Run docker compose with both compose files
run_compose() {
    local compose_cmd=$(get_compose_cmd)
    cd "$DOCKER_DIR"
    $compose_cmd \
        -f docker-compose.yml \
        -f docker-compose.distributed.yml \
        --env-file "$ENV_FILE" \
        -p "$COMPOSE_PROJECT_NAME" \
        "$@"
}

# Start services
cmd_start() {
    log_info "Starting distributed crawler services..."

    check_prerequisites

    # Start infrastructure first
    log_info "Starting infrastructure (PostgreSQL, Redis, OpenSearch)..."
    run_compose up -d postgres redis opensearch

    # Wait for infrastructure to be ready
    log_info "Waiting for infrastructure to be healthy..."
    sleep 10

    # Start coordinator
    log_info "Starting coordinator server..."
    run_compose up -d coordinator

    # Wait for coordinator
    sleep 5

    # Start crawlers
    log_info "Starting crawler instances..."
    run_compose up -d crawler-main crawler-sub1 crawler-sub2

    log_success "All services started successfully"

    # Show status
    cmd_status
}

# Stop services
cmd_stop() {
    log_info "Stopping distributed crawler services..."

    # Stop in reverse order
    log_info "Stopping crawlers..."
    run_compose stop crawler-main crawler-sub1 crawler-sub2

    log_info "Stopping coordinator..."
    run_compose stop coordinator

    log_info "Stopping infrastructure..."
    run_compose stop

    log_success "All services stopped"
}

# Restart services
cmd_restart() {
    log_info "Restarting services..."
    cmd_stop
    sleep 2
    cmd_start
}

# Show status
cmd_status() {
    log_info "Service Status:"
    echo ""
    run_compose ps
    echo ""

    # Show health status
    log_info "Health Status:"
    echo ""

    # Check coordinator
    if curl -sf "http://localhost:${COORDINATOR_PORT:-8000}/health" > /dev/null 2>&1; then
        echo -e "  Coordinator: ${GREEN}healthy${NC}"
    else
        echo -e "  Coordinator: ${RED}unhealthy${NC}"
    fi

    # Check PostgreSQL
    if docker exec ntimes-postgres pg_isready -U ntimes > /dev/null 2>&1; then
        echo -e "  PostgreSQL:  ${GREEN}healthy${NC}"
    else
        echo -e "  PostgreSQL:  ${RED}unhealthy${NC}"
    fi

    # Check Redis
    if docker exec ntimes-redis redis-cli ping > /dev/null 2>&1; then
        echo -e "  Redis:       ${GREEN}healthy${NC}"
    else
        echo -e "  Redis:       ${RED}unhealthy${NC}"
    fi

    # Check OpenSearch
    if curl -sf "http://localhost:${OPENSEARCH_PORT:-9200}/_cluster/health" > /dev/null 2>&1; then
        echo -e "  OpenSearch:  ${GREEN}healthy${NC}"
    else
        echo -e "  OpenSearch:  ${RED}unhealthy${NC}"
    fi

    echo ""
}

# Show logs
cmd_logs() {
    local service="${1:-}"

    if [ -z "$service" ]; then
        log_info "Following logs for all services..."
        run_compose logs -f --tail=100
    else
        log_info "Following logs for $service..."
        run_compose logs -f --tail=100 "$service"
    fi
}

# Build images
cmd_build() {
    log_info "Building Docker images..."

    cd "$PROJECT_ROOT"
    docker build -t ntimes:latest .

    log_success "Image built successfully"
}

# Scale crawlers
cmd_scale() {
    local count="${1:-3}"

    log_info "Scaling crawler instances to $count..."
    log_warn "Note: Only 3 instances (main, sub1, sub2) are supported in the current scheduler"

    if [ "$count" -gt 3 ]; then
        log_warn "Maximum 3 instances supported. Using 3."
        count=3
    fi

    # Enable/disable instances based on count
    if [ "$count" -ge 1 ]; then
        run_compose up -d crawler-main
    else
        run_compose stop crawler-main
    fi

    if [ "$count" -ge 2 ]; then
        run_compose up -d crawler-sub1
    else
        run_compose stop crawler-sub1
    fi

    if [ "$count" -ge 3 ]; then
        run_compose up -d crawler-sub2
    else
        run_compose stop crawler-sub2
    fi

    log_success "Scaled to $count crawler instances"
}

# Health check
cmd_health() {
    log_info "Running health checks..."

    local all_healthy=true

    # Check coordinator health endpoint
    echo "Checking coordinator..."
    if curl -sf "http://localhost:${COORDINATOR_PORT:-8000}/health" | jq . 2>/dev/null; then
        echo -e "${GREEN}Coordinator: OK${NC}"
    else
        echo -e "${RED}Coordinator: FAILED${NC}"
        all_healthy=false
    fi

    # Check scheduler status
    echo ""
    echo "Checking schedule status..."
    if curl -sf "http://localhost:${COORDINATOR_PORT:-8000}/schedule" | jq . 2>/dev/null; then
        echo -e "${GREEN}Schedule: OK${NC}"
    else
        echo -e "${RED}Schedule: FAILED${NC}"
        all_healthy=false
    fi

    # Check registered instances
    echo ""
    echo "Checking registered instances..."
    if curl -sf "http://localhost:${COORDINATOR_PORT:-8000}/instances" | jq . 2>/dev/null; then
        echo -e "${GREEN}Instances: OK${NC}"
    else
        echo -e "${RED}Instances: FAILED${NC}"
        all_healthy=false
    fi

    echo ""
    if $all_healthy; then
        log_success "All health checks passed"
    else
        log_error "Some health checks failed"
        exit 1
    fi
}

# Clean up
cmd_clean() {
    log_warn "This will remove all containers and volumes!"
    read -p "Are you sure? (y/N) " -n 1 -r
    echo

    if [[ $REPLY =~ ^[Yy]$ ]]; then
        log_info "Stopping and removing all services..."
        run_compose down -v --remove-orphans

        log_info "Removing dangling images..."
        docker image prune -f

        log_success "Cleanup complete"
    else
        log_info "Cleanup cancelled"
    fi
}

# Show help
show_help() {
    cat << EOF
nTimes Distributed Crawler Deployment Script

Usage: $0 [command] [options]

Commands:
  start           Start all services (coordinator + 3 crawlers)
  stop            Stop all services
  restart         Restart all services
  status          Show service status
  logs [service]  Show logs (follow mode). Optionally specify service.
  build           Build Docker images
  scale <count>   Scale crawler instances (1-3)
  health          Check health of all services
  clean           Remove all containers and volumes
  help            Show this help message

Environment Variables:
  COMPOSE_PROJECT_NAME  Project name (default: ntimes)
  ENV_FILE              Environment file path (default: docker/.env)
  COORDINATOR_PORT      Coordinator port (default: 8000)

Examples:
  $0 start                    # Start all services
  $0 logs coordinator         # Follow coordinator logs
  $0 scale 2                  # Run only 2 crawler instances
  $0 health                   # Check system health

For more information, see: https://github.com/hephaex/nTimes
EOF
}

# Main
main() {
    local command="${1:-help}"
    shift || true

    case "$command" in
        start)
            cmd_start
            ;;
        stop)
            cmd_stop
            ;;
        restart)
            cmd_restart
            ;;
        status)
            cmd_status
            ;;
        logs)
            cmd_logs "$@"
            ;;
        build)
            cmd_build
            ;;
        scale)
            cmd_scale "$@"
            ;;
        health)
            cmd_health
            ;;
        clean)
            cmd_clean
            ;;
        help|--help|-h)
            show_help
            ;;
        *)
            log_error "Unknown command: $command"
            show_help
            exit 1
            ;;
    esac
}

main "$@"
