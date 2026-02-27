#!/bin/bash
# =============================================================================
# Baram Alpine VM Bootstrap Script
# =============================================================================
# Run this script on a fresh Alpine Linux 3.21 VM after setup-alpine.
# Must be run as root.
#
# Usage: ssh root@alpine-vm 'bash -s' < scripts/alpine-migration/01-bootstrap.sh
# =============================================================================
set -euo pipefail

# --- Configuration ---
USERNAME="${BARAM_USER:-mare}"
BARAM_REPO="${BARAM_REPO:-https://github.com/hephaex/baram.git}"
NODE_VERSION="lts"  # Alpine community repo tracks LTS

echo "============================================="
echo " Baram Alpine VM Bootstrap"
echo " Alpine $(cat /etc/alpine-release 2>/dev/null || echo 'unknown')"
echo "============================================="

# --- Step 1: Enable community repository ---
echo "[1/9] Enabling community repository..."
ALPINE_VER=$(cat /etc/alpine-release | cut -d. -f1,2)
REPO_FILE="/etc/apk/repositories"

if ! grep -q "community" "$REPO_FILE" 2>/dev/null; then
    echo "http://dl-cdn.alpinelinux.org/alpine/v${ALPINE_VER}/community" >> "$REPO_FILE"
fi

apk update && apk upgrade

# --- Step 2: Install base packages ---
echo "[2/9] Installing base packages..."
apk add \
    bash \
    sudo \
    curl \
    wget \
    git \
    openssh \
    openrc \
    shadow \
    coreutils \
    util-linux \
    flock \
    rsync \
    jq \
    htop \
    tmux \
    ca-certificates

# --- Step 3: Create developer user ---
echo "[3/9] Setting up user '${USERNAME}'..."
if ! id "$USERNAME" &>/dev/null; then
    adduser -D -s /bin/bash "$USERNAME"
    echo "${USERNAME} ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers
    echo "  -> User '${USERNAME}' created with sudo access"
else
    echo "  -> User '${USERNAME}' already exists, skipping"
fi

# --- Step 4: Docker + Docker Compose ---
echo "[4/9] Installing Docker..."
apk add docker docker-cli-compose

rc-update add docker default
service docker start || true

# Add user to docker group
addgroup "$USERNAME" docker 2>/dev/null || true

# Verify docker
if docker info &>/dev/null; then
    echo "  -> Docker running: $(docker --version)"
else
    echo "  !! Docker not running, may need reboot"
fi

# --- Step 5: Rust build dependencies (for musl) ---
echo "[5/9] Installing Rust build dependencies..."
apk add \
    build-base \
    cmake \
    pkgconf \
    openssl-dev \
    postgresql-dev \
    musl-dev \
    perl \
    linux-headers \
    protobuf-dev \
    sqlite-dev

# --- Step 6: Install Rust as user ---
echo "[6/9] Installing Rust toolchain for '${USERNAME}'..."
su - "$USERNAME" -c '
    if command -v rustc &>/dev/null; then
        echo "  -> Rust already installed: $(rustc --version)"
    else
        curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
        echo "  -> Rust installed: $(rustc --version)"
    fi
'

# --- Step 7: Node.js (for Claude Code + frontend) ---
echo "[7/9] Installing Node.js..."
apk add nodejs npm

echo "  -> Node.js: $(node --version)"
echo "  -> npm: $(npm --version)"

# --- Step 8: Claude Code ---
echo "[8/9] Installing Claude Code..."
npm install -g @anthropic-ai/claude-code 2>/dev/null || {
    echo "  !! Claude Code install failed (may need newer npm)"
    echo "  -> Try: npm install -g @anthropic-ai/claude-code --force"
}

# --- Step 9: Cron (dcron) ---
echo "[9/9] Installing cron daemon..."
apk add dcron
rc-update add dcron default
service dcron start || true

# --- Post-setup: Shell config ---
echo ""
echo "Setting up bash profile for ${USERNAME}..."
su - "$USERNAME" -c '
cat >> ~/.bashrc << '\''BASHRC'\''
# Baram environment
export PATH="$HOME/.cargo/bin:$PATH"
export BARAM_HOME="$HOME/Baram"

# OpenSearch
export OPENSEARCH_URL=http://localhost:9200
export OPENSEARCH_INDEX=baram-articles

# Embedding server
export EMBEDDING_SERVER_URL=http://localhost:8090

# Rust logging
export RUST_LOG=info

# Aliases
alias bb="cd ~/Baram"
alias bc="cd ~/Baram && cargo"
alias bl="tail -f ~/Baram/logs/*.log"
BASHRC
'

# --- Summary ---
echo ""
echo "============================================="
echo " Bootstrap Complete!"
echo "============================================="
echo ""
echo " Next steps:"
echo "   1. Set ANTHROPIC_API_KEY in ~${USERNAME}/.bashrc"
echo "   2. Clone the project:"
echo "      su - ${USERNAME}"
echo "      git clone ${BARAM_REPO} ~/Baram"
echo "   3. Build:"
echo "      cd ~/Baram && cargo build --release"
echo "   4. Run data migration:"
echo "      bash scripts/alpine-migration/02-migrate-data.sh"
echo "   5. Start Docker services:"
echo "      cd docker && docker compose up -d"
echo "   6. Set up cron + OpenRC:"
echo "      sudo bash scripts/alpine-migration/03-services.sh"
echo "   7. Verify:"
echo "      bash scripts/alpine-migration/04-verify.sh"
echo ""
echo " musl build troubleshooting:"
echo '   - OpenSSL link error → export OPENSSL_DIR=/usr'
echo '   - candle build fail  → RUSTFLAGS="-C target-feature=-crt-static"'
echo '   - tokenizers cmake   → apk add protobuf-dev (already installed)'
echo "============================================="
