#!/bin/bash
# nTimes Docker Environment Setup Script
# Copyright (c) 2024 hephaex@gmail.com
# License: GPL v3

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Functions
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_command() {
    if ! command -v $1 &> /dev/null; then
        print_error "$1 is not installed. Please install it first."
        exit 1
    fi
}

generate_password() {
    # Generate a secure random password
    local length=${1:-24}
    LC_ALL=C tr -dc 'A-Za-z0-9!@#$%^&*' < /dev/urandom | head -c $length
}

# Header
echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║         nTimes Docker Environment Setup                   ║"
echo "║         Naver News Crawler Infrastructure                 ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# Check prerequisites
print_info "Checking prerequisites..."
check_command docker
check_command docker-compose
print_success "Docker and Docker Compose are installed"

# Check Docker daemon
if ! docker info &> /dev/null; then
    print_error "Docker daemon is not running. Please start Docker first."
    exit 1
fi
print_success "Docker daemon is running"

# Check system requirements
print_info "Checking system requirements..."

# Check vm.max_map_count for OpenSearch
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    current_max_map=$(sysctl -n vm.max_map_count)
    if [ "$current_max_map" -lt 262144 ]; then
        print_warning "vm.max_map_count is too low ($current_max_map)"
        print_info "Attempting to increase vm.max_map_count..."

        if sudo sysctl -w vm.max_map_count=262144 &> /dev/null; then
            print_success "Increased vm.max_map_count to 262144"

            # Make it permanent
            if ! grep -q "vm.max_map_count=262144" /etc/sysctl.conf 2>/dev/null; then
                echo "vm.max_map_count=262144" | sudo tee -a /etc/sysctl.conf > /dev/null
                print_success "Made vm.max_map_count change permanent"
            fi
        else
            print_error "Failed to increase vm.max_map_count. Please run manually:"
            print_error "  sudo sysctl -w vm.max_map_count=262144"
            exit 1
        fi
    else
        print_success "vm.max_map_count is sufficient ($current_max_map)"
    fi
fi

# Navigate to docker directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

# Check if .env exists
if [ -f ".env" ]; then
    print_warning ".env file already exists"
    read -p "Do you want to regenerate it? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Keeping existing .env file"
        ENV_EXISTS=true
    else
        ENV_EXISTS=false
    fi
else
    ENV_EXISTS=false
fi

# Create .env file
if [ "$ENV_EXISTS" = false ]; then
    print_info "Creating .env file from template..."
    cp .env.example .env

    # Generate secure passwords
    print_info "Generating secure passwords..."
    POSTGRES_PASS=$(generate_password 24)
    OPENSEARCH_PASS="Admin$(generate_password 16)!"

    # Update .env with generated passwords
    sed -i.bak "s/POSTGRES_PASSWORD=.*/POSTGRES_PASSWORD=$POSTGRES_PASS/" .env
    sed -i.bak "s/OPENSEARCH_INITIAL_ADMIN_PASSWORD=.*/OPENSEARCH_INITIAL_ADMIN_PASSWORD=$OPENSEARCH_PASS/" .env
    rm -f .env.bak

    print_success "Created .env file with secure passwords"
    print_warning "Your passwords have been saved to .env file"
    print_warning "Keep this file secure and do NOT commit it to version control"
fi

# Create config.toml in parent directory
cd ..
if [ ! -f "config.toml" ]; then
    print_info "Creating config.toml from template..."
    cp config.toml.example config.toml

    # Update config.toml with passwords from .env
    if [ -f "docker/.env" ]; then
        source docker/.env
        sed -i.bak "s/password = \"changeme_strong_password_here\"/password = \"$POSTGRES_PASSWORD\"/" config.toml
        sed -i.bak "s/password = \"Admin123!ChangeMeNow\"/password = \"$OPENSEARCH_INITIAL_ADMIN_PASSWORD\"/" config.toml
        rm -f config.toml.bak
    fi

    print_success "Created config.toml"
else
    print_info "config.toml already exists"
fi

# Create output directories
print_info "Creating output directories..."
mkdir -p output/raw output/markdown checkpoints logs models
print_success "Created output directories"

# Go back to docker directory
cd docker

# Pull Docker images
print_info "Pulling Docker images (this may take a few minutes)..."
docker-compose pull
print_success "Docker images pulled"

# Start services
print_info "Starting Docker services..."
docker-compose up -d postgres opensearch redis
print_success "Services started"

# Wait for services to be healthy
print_info "Waiting for services to be healthy (this may take 30-60 seconds)..."
sleep 10

# Function to check service health
check_health() {
    local service=$1
    local max_attempts=30
    local attempt=0

    while [ $attempt -lt $max_attempts ]; do
        if docker-compose ps $service | grep -q "Up (healthy)"; then
            return 0
        fi
        sleep 2
        attempt=$((attempt + 1))
    done
    return 1
}

# Check PostgreSQL
print_info "Checking PostgreSQL..."
if check_health postgres; then
    print_success "PostgreSQL is healthy"
else
    print_error "PostgreSQL failed to start. Check logs: docker-compose logs postgres"
    exit 1
fi

# Check OpenSearch
print_info "Checking OpenSearch..."
if check_health opensearch; then
    print_success "OpenSearch is healthy"
else
    print_error "OpenSearch failed to start. Check logs: docker-compose logs opensearch"
    exit 1
fi

# Check Redis
print_info "Checking Redis..."
if check_health redis; then
    print_success "Redis is healthy"
else
    print_error "Redis failed to start. Check logs: docker-compose logs redis"
    exit 1
fi

# Create OpenSearch index
print_info "Creating OpenSearch index..."
source .env
sleep 5  # Give OpenSearch a bit more time

OPENSEARCH_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" -X PUT "localhost:9200/naver-news" \
    -u "admin:$OPENSEARCH_INITIAL_ADMIN_PASSWORD" \
    -H 'Content-Type: application/json' \
    -d @opensearch-index-template.json)

if [ "$OPENSEARCH_RESPONSE" = "200" ]; then
    print_success "OpenSearch index 'naver-news' created"
elif [ "$OPENSEARCH_RESPONSE" = "400" ]; then
    print_warning "OpenSearch index already exists"
else
    print_warning "Failed to create OpenSearch index (HTTP $OPENSEARCH_RESPONSE)"
    print_info "You can create it manually later with: make opensearch-create-index"
fi

# Display status
echo ""
print_success "Setup complete! 🎉"
echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║                    Service Status                         ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
docker-compose ps
echo ""

# Display connection info
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║                 Connection Information                    ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "PostgreSQL:"
echo "  Host:     localhost:5432"
echo "  Database: ntimes"
echo "  Username: ntimes"
echo "  Password: (see docker/.env)"
echo ""
echo "OpenSearch:"
echo "  URL:      http://localhost:9200"
echo "  Username: admin"
echo "  Password: (see docker/.env)"
echo ""
echo "Redis:"
echo "  URL:      redis://localhost:6379"
echo ""

# Next steps
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║                      Next Steps                           ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "1. Build the Rust application:"
echo "   cargo build --release"
echo ""
echo "2. Run your first crawl:"
echo "   cargo run --release -- crawl --category politics --max-articles 10"
echo ""
echo "3. Start development tools (optional):"
echo "   cd docker && docker-compose --profile development up -d"
echo "   - pgAdmin: http://localhost:5050"
echo "   - OpenSearch Dashboards: http://localhost:5601"
echo ""
echo "4. View logs:"
echo "   docker-compose logs -f"
echo ""
echo "For more information, see DOCKER_SETUP.md"
echo ""
print_success "Happy crawling! 🚀"
