# Makefile for nTimes Naver News Crawler
# Copyright (c) 2024 hephaex@gmail.com
# License: GPL v3

.PHONY: help setup start stop restart logs clean build test dev-tools backup restore

# Default target
.DEFAULT_GOAL := help

# Colors for output
COLOR_RESET   = \033[0m
COLOR_INFO    = \033[36m
COLOR_SUCCESS = \033[32m
COLOR_WARNING = \033[33m
COLOR_ERROR   = \033[31m

# ============================================================================
# Help
# ============================================================================

help: ## Display this help message
	@echo "$(COLOR_INFO)nTimes Naver News Crawler - Available Commands$(COLOR_RESET)"
	@echo ""
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  $(COLOR_SUCCESS)%-20s$(COLOR_RESET) %s\n", $$1, $$2}'
	@echo ""

# ============================================================================
# Setup and Configuration
# ============================================================================

setup: ## Initial setup - copy env files and create directories
	@echo "$(COLOR_INFO)Setting up nTimes environment...$(COLOR_RESET)"
	@if [ ! -f docker/.env ]; then \
		cp docker/.env.example docker/.env; \
		echo "$(COLOR_WARNING)Created docker/.env - Please edit and set passwords!$(COLOR_RESET)"; \
	else \
		echo "$(COLOR_SUCCESS)docker/.env already exists$(COLOR_RESET)"; \
	fi
	@if [ ! -f config.toml ]; then \
		cp config.toml.example config.toml; \
		echo "$(COLOR_SUCCESS)Created config.toml$(COLOR_RESET)"; \
	else \
		echo "$(COLOR_SUCCESS)config.toml already exists$(COLOR_RESET)"; \
	fi
	@mkdir -p output/raw output/markdown checkpoints logs models
	@echo "$(COLOR_SUCCESS)Created output directories$(COLOR_RESET)"
	@echo "$(COLOR_INFO)Setup complete! Next steps:$(COLOR_RESET)"
	@echo "  1. Edit docker/.env and set secure passwords"
	@echo "  2. Run 'make start' to start services"
	@echo "  3. Run 'make dev-tools' to start development tools"

check-env: ## Check if environment file exists
	@if [ ! -f docker/.env ]; then \
		echo "$(COLOR_ERROR)Error: docker/.env not found!$(COLOR_RESET)"; \
		echo "Run 'make setup' first"; \
		exit 1; \
	fi

# ============================================================================
# Docker Services
# ============================================================================

start: check-env ## Start core services (PostgreSQL, OpenSearch, Redis)
	@echo "$(COLOR_INFO)Starting core services...$(COLOR_RESET)"
	cd docker && docker-compose up -d postgres opensearch redis
	@echo "$(COLOR_SUCCESS)Services started!$(COLOR_RESET)"
	@echo "Waiting for services to be healthy..."
	@sleep 5
	@make status

dev-tools: check-env ## Start development tools (pgAdmin, OpenSearch Dashboards)
	@echo "$(COLOR_INFO)Starting development tools...$(COLOR_RESET)"
	cd docker && docker-compose --profile development up -d
	@echo "$(COLOR_SUCCESS)Development tools started!$(COLOR_RESET)"
	@echo "  - pgAdmin: http://localhost:5050"
	@echo "  - OpenSearch Dashboards: http://localhost:5601"

stop: ## Stop all services
	@echo "$(COLOR_INFO)Stopping services...$(COLOR_RESET)"
	cd docker && docker-compose down
	@echo "$(COLOR_SUCCESS)Services stopped$(COLOR_RESET)"

restart: ## Restart all services
	@echo "$(COLOR_INFO)Restarting services...$(COLOR_RESET)"
	cd docker && docker-compose restart
	@echo "$(COLOR_SUCCESS)Services restarted$(COLOR_RESET)"

status: ## Show service status
	@echo "$(COLOR_INFO)Service Status:$(COLOR_RESET)"
	@cd docker && docker-compose ps

# ============================================================================
# Logs
# ============================================================================

logs: ## Tail logs from all services
	cd docker && docker-compose logs -f

logs-postgres: ## Tail PostgreSQL logs
	cd docker && docker-compose logs -f postgres

logs-opensearch: ## Tail OpenSearch logs
	cd docker && docker-compose logs -f opensearch

logs-redis: ## Tail Redis logs
	cd docker && docker-compose logs -f redis

# ============================================================================
# Database Operations
# ============================================================================

db-shell: ## Open PostgreSQL shell
	cd docker && docker-compose exec postgres psql -U ntimes -d ntimes

db-reset: ## Reset database (WARNING: deletes all data)
	@echo "$(COLOR_WARNING)WARNING: This will delete all data!$(COLOR_RESET)"
	@read -p "Are you sure? [y/N] " -n 1 -r; \
	echo; \
	if [[ $$REPLY =~ ^[Yy]$$ ]]; then \
		echo "$(COLOR_INFO)Resetting database...$(COLOR_RESET)"; \
		cd docker && docker-compose down -v postgres; \
		cd docker && docker-compose up -d postgres; \
		echo "$(COLOR_SUCCESS)Database reset complete$(COLOR_RESET)"; \
	else \
		echo "$(COLOR_INFO)Cancelled$(COLOR_RESET)"; \
	fi

db-migrate: ## Run database migrations (placeholder)
	@echo "$(COLOR_INFO)Running migrations...$(COLOR_RESET)"
	@echo "$(COLOR_WARNING)Not implemented yet$(COLOR_RESET)"

# ============================================================================
# OpenSearch Operations
# ============================================================================

opensearch-shell: ## Open OpenSearch shell (curl-based)
	@echo "$(COLOR_INFO)OpenSearch Cluster Health:$(COLOR_RESET)"
	@curl -s -u admin:$$(grep OPENSEARCH_INITIAL_ADMIN_PASSWORD docker/.env | cut -d '=' -f2) \
		http://localhost:9200/_cluster/health?pretty

opensearch-indices: ## List OpenSearch indices
	@curl -s -u admin:$$(grep OPENSEARCH_INITIAL_ADMIN_PASSWORD docker/.env | cut -d '=' -f2) \
		http://localhost:9200/_cat/indices?v

opensearch-create-index: ## Create naver-news index with Korean analyzer
	@echo "$(COLOR_INFO)Creating naver-news index...$(COLOR_RESET)"
	@curl -X PUT "localhost:9200/naver-news" \
		-u admin:$$(grep OPENSEARCH_INITIAL_ADMIN_PASSWORD docker/.env | cut -d '=' -f2) \
		-H 'Content-Type: application/json' \
		-d @docker/opensearch-index-template.json
	@echo "\n$(COLOR_SUCCESS)Index created$(COLOR_RESET)"

opensearch-delete-index: ## Delete naver-news index (WARNING: deletes all data)
	@echo "$(COLOR_WARNING)WARNING: This will delete the index and all data!$(COLOR_RESET)"
	@read -p "Are you sure? [y/N] " -n 1 -r; \
	echo; \
	if [[ $$REPLY =~ ^[Yy]$$ ]]; then \
		curl -X DELETE "localhost:9200/naver-news" \
			-u admin:$$(grep OPENSEARCH_INITIAL_ADMIN_PASSWORD docker/.env | cut -d '=' -f2); \
		echo "\n$(COLOR_SUCCESS)Index deleted$(COLOR_RESET)"; \
	else \
		echo "$(COLOR_INFO)Cancelled$(COLOR_RESET)"; \
	fi

# ============================================================================
# Redis Operations
# ============================================================================

redis-shell: ## Open Redis CLI
	cd docker && docker-compose exec redis redis-cli

redis-flush: ## Flush all Redis data (WARNING: deletes all cache)
	@echo "$(COLOR_WARNING)WARNING: This will delete all Redis data!$(COLOR_RESET)"
	@read -p "Are you sure? [y/N] " -n 1 -r; \
	echo; \
	if [[ $$REPLY =~ ^[Yy]$$ ]]; then \
		cd docker && docker-compose exec redis redis-cli FLUSHALL; \
		echo "$(COLOR_SUCCESS)Redis flushed$(COLOR_RESET)"; \
	else \
		echo "$(COLOR_INFO)Cancelled$(COLOR_RESET)"; \
	fi

# ============================================================================
# Backup and Restore
# ============================================================================

backup: ## Backup PostgreSQL and OpenSearch data
	@echo "$(COLOR_INFO)Creating backup...$(COLOR_RESET)"
	@mkdir -p backups
	@TIMESTAMP=$$(date +%Y%m%d_%H%M%S); \
	cd docker && docker-compose exec -T postgres pg_dump -U ntimes ntimes > ../backups/postgres_$$TIMESTAMP.sql; \
	echo "$(COLOR_SUCCESS)PostgreSQL backup created: backups/postgres_$$TIMESTAMP.sql$(COLOR_RESET)"

restore: ## Restore PostgreSQL from backup (Usage: make restore FILE=backups/postgres_20240115.sql)
	@if [ -z "$(FILE)" ]; then \
		echo "$(COLOR_ERROR)Error: FILE parameter required$(COLOR_RESET)"; \
		echo "Usage: make restore FILE=backups/postgres_20240115.sql"; \
		exit 1; \
	fi
	@if [ ! -f "$(FILE)" ]; then \
		echo "$(COLOR_ERROR)Error: File not found: $(FILE)$(COLOR_RESET)"; \
		exit 1; \
	fi
	@echo "$(COLOR_WARNING)This will overwrite the current database!$(COLOR_RESET)"
	@read -p "Continue? [y/N] " -n 1 -r; \
	echo; \
	if [[ $$REPLY =~ ^[Yy]$$ ]]; then \
		cd docker && docker-compose exec -T postgres psql -U ntimes ntimes < ../$(FILE); \
		echo "$(COLOR_SUCCESS)Database restored from $(FILE)$(COLOR_RESET)"; \
	else \
		echo "$(COLOR_INFO)Cancelled$(COLOR_RESET)"; \
	fi

# ============================================================================
# Build and Test
# ============================================================================

build: ## Build Rust application
	@echo "$(COLOR_INFO)Building Rust application...$(COLOR_RESET)"
	cargo build --release
	@echo "$(COLOR_SUCCESS)Build complete$(COLOR_RESET)"

test: ## Run tests
	@echo "$(COLOR_INFO)Running tests...$(COLOR_RESET)"
	cargo test
	@echo "$(COLOR_SUCCESS)Tests complete$(COLOR_RESET)"

test-integration: start ## Run integration tests with Docker services
	@echo "$(COLOR_INFO)Running integration tests...$(COLOR_RESET)"
	@sleep 5  # Wait for services to be ready
	cargo test --test '*' -- --test-threads=1
	@echo "$(COLOR_SUCCESS)Integration tests complete$(COLOR_RESET)"

lint: ## Run clippy linter
	@echo "$(COLOR_INFO)Running clippy...$(COLOR_RESET)"
	cargo clippy --all-targets --all-features -- -D warnings

format: ## Format code with rustfmt
	@echo "$(COLOR_INFO)Formatting code...$(COLOR_RESET)"
	cargo fmt

format-check: ## Check code formatting
	@echo "$(COLOR_INFO)Checking code format...$(COLOR_RESET)"
	cargo fmt -- --check

# ============================================================================
# Docker Image
# ============================================================================

docker-build: ## Build Docker image
	@echo "$(COLOR_INFO)Building Docker image...$(COLOR_RESET)"
	docker build -t ntimes:latest .
	@echo "$(COLOR_SUCCESS)Docker image built: ntimes:latest$(COLOR_RESET)"

docker-run: ## Run crawler in Docker container
	@echo "$(COLOR_INFO)Running crawler in Docker...$(COLOR_RESET)"
	docker run --rm -it \
		--network host \
		--env-file docker/.env \
		-v $$(pwd)/output:/app/output \
		-v $$(pwd)/checkpoints:/app/checkpoints \
		ntimes:latest $(ARGS)

# ============================================================================
# Cleanup
# ============================================================================

clean: ## Clean build artifacts
	@echo "$(COLOR_INFO)Cleaning build artifacts...$(COLOR_RESET)"
	cargo clean
	rm -rf target/
	@echo "$(COLOR_SUCCESS)Clean complete$(COLOR_RESET)"

clean-all: stop clean ## Stop services and clean everything
	@echo "$(COLOR_WARNING)Removing all Docker volumes (WARNING: deletes all data)$(COLOR_RESET)"
	@read -p "Are you sure? [y/N] " -n 1 -r; \
	echo; \
	if [[ $$REPLY =~ ^[Yy]$$ ]]; then \
		cd docker && docker-compose down -v; \
		rm -rf output/ checkpoints/ logs/ models/; \
		echo "$(COLOR_SUCCESS)Everything cleaned$(COLOR_RESET)"; \
	else \
		echo "$(COLOR_INFO)Cancelled$(COLOR_RESET)"; \
	fi

# ============================================================================
# Development Helpers
# ============================================================================

dev: setup start dev-tools ## Complete development setup
	@echo "$(COLOR_SUCCESS)Development environment ready!$(COLOR_RESET)"
	@make status

watch: ## Watch and rebuild on file changes (requires cargo-watch)
	@echo "$(COLOR_INFO)Watching for changes...$(COLOR_RESET)"
	cargo watch -x build

run-crawl: ## Run crawler (example: make run-crawl ARGS="--category politics --max-articles 10")
	@echo "$(COLOR_INFO)Running crawler...$(COLOR_RESET)"
	cargo run --release -- crawl $(ARGS)

run-search: ## Run search (example: make run-search ARGS="반도체")
	@echo "$(COLOR_INFO)Running search...$(COLOR_RESET)"
	cargo run --release -- search $(ARGS)

# ============================================================================
# CI/CD
# ============================================================================

ci: format-check lint test ## Run CI checks (format, lint, test)
	@echo "$(COLOR_SUCCESS)All CI checks passed!$(COLOR_RESET)"

pre-commit: format lint test ## Run pre-commit checks
	@echo "$(COLOR_SUCCESS)Pre-commit checks passed!$(COLOR_RESET)"
