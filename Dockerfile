# Dockerfile for Baram Naver News Crawler
# Multi-stage build for optimized production image
# Copyright (c) 2024 hephaex@gmail.com
# License: GPL v3

# ============================================================================
# Stage 1: Builder - Compile Rust application
# ============================================================================
FROM rust:1.83-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    ca-certificates \
    build-essential \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for building
RUN useradd -m -u 1001 -s /bin/bash builder

# Set working directory
WORKDIR /build

# Copy dependency manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy source to cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY --chown=builder:builder . .

# Build the application with optimizations
RUN cargo build --release --locked

# Strip debug symbols to reduce binary size
RUN strip /build/target/release/baram

# ============================================================================
# Stage 2: Runtime - Minimal production image
# ============================================================================
FROM debian:bookworm-slim

# Install runtime dependencies only
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user and group for running the application
RUN groupadd -r -g 1001 baram && \
    useradd -r -u 1001 -g baram -s /bin/bash -m -d /app baram

# Create necessary directories with proper permissions
RUN mkdir -p /app/output/raw \
    /app/output/markdown \
    /app/checkpoints \
    /app/logs \
    /app/models \
    && chown -R baram:baram /app

# Set working directory
WORKDIR /app

# Copy the compiled binary from builder stage
COPY --from=builder --chown=baram:baram /build/target/release/baram /usr/local/bin/baram

# Copy configuration files
COPY --chown=baram:baram config.toml.example /app/config.toml

# Set environment variables
ENV RUST_LOG=info \
    RUST_BACKTRACE=1 \
    OUTPUT_DIR=/app/output \
    CHECKPOINT_DIR=/app/checkpoints \
    LOG_FILE=/app/logs/baram.log \
    HF_HOME=/app/models

# Switch to non-root user
USER baram

# Expose health check port (if implemented)
EXPOSE 8080

# Health check - uses liveness probe endpoint for Docker health checks
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health/live || exit 1

# Default command (can be overridden)
ENTRYPOINT ["/usr/local/bin/baram"]
CMD ["--help"]

# Labels for image metadata
LABEL maintainer="hephaex@gmail.com" \
      version="0.1.0" \
      description="Baram Naver News Crawler - Rust-based high-performance crawler" \
      license="GPL-3.0" \
      org.opencontainers.image.source="https://github.com/hephaex/baram" \
      org.opencontainers.image.documentation="https://github.com/hephaex/baram/blob/main/README.md"

# ============================================================================
# Build instructions:
# ============================================================================
# Build the image:
#   docker build -t baram:latest .
#
# Build with specific Rust version:
#   docker build --build-arg RUST_VERSION=1.75 -t baram:latest .
#
# Run the container:
#   docker run --rm -it \
#     --env-file docker/.env \
#     -v $(pwd)/output:/app/output \
#     -v $(pwd)/checkpoints:/app/checkpoints \
#     baram:latest crawl --category politics --max-articles 100
#
# Run with docker-compose:
#   docker-compose up -d
#   docker-compose exec crawler baram crawl --category politics
#
# ============================================================================
# Production recommendations:
# ============================================================================
# 1. Use specific version tags instead of :latest
# 2. Scan images for vulnerabilities (e.g., trivy, snyk)
# 3. Sign images for supply chain security
# 4. Use read-only root filesystem where possible
# 5. Implement proper secrets management
# 6. Set resource limits (memory, CPU)
# 7. Configure log rotation
# 8. Monitor container metrics
# 9. Use private registry for production images
# 10. Implement automated security patching
