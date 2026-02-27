#!/bin/bash
# =============================================================================
# Baram Alpine Services Setup
# =============================================================================
# Creates OpenRC services and cron jobs to replace systemd timers.
# Must be run as root on the Alpine VM.
#
# Usage: sudo bash scripts/alpine-migration/03-services.sh
# =============================================================================
set -euo pipefail

USERNAME="${BARAM_USER:-mare}"
BARAM_DIR="/home/${USERNAME}/Baram"

echo "============================================="
echo " Baram Alpine Services Setup"
echo "============================================="

# --- Verify prerequisites ---
if [ "$(id -u)" -ne 0 ]; then
    echo "ERROR: Must run as root"
    exit 1
fi

if [ ! -d "$BARAM_DIR" ]; then
    echo "ERROR: ${BARAM_DIR} not found"
    exit 1
fi

# --- 1. Create baram-embedding OpenRC service ---
echo "[1/4] Creating baram-embedding OpenRC service..."

cat > /etc/init.d/baram-embedding << 'INITD'
#!/sbin/openrc-run

name="baram-embedding"
description="Baram Embedding Server"
command="/home/mare/Baram/target/release/baram"
command_args="embedding-server --port 8090"
command_user="mare"
command_background=true
pidfile="/run/${RC_SVCNAME}.pid"
output_log="/home/mare/Baram/logs/embedding.log"
error_log="/home/mare/Baram/logs/embedding-error.log"

depend() {
    need net
    after docker
}

start_pre() {
    checkpath -d -o mare:mare /home/mare/Baram/logs
}
INITD

chmod +x /etc/init.d/baram-embedding
echo "  -> /etc/init.d/baram-embedding created"

# --- 2. Create baram-api OpenRC service ---
echo "[2/4] Creating baram-api OpenRC service..."

cat > /etc/init.d/baram-api << 'INITD'
#!/sbin/openrc-run

name="baram-api"
description="Baram REST API Server"
command="/home/mare/Baram/target/release/baram"
command_args="serve --port 8080"
command_user="mare"
command_background=true
pidfile="/run/${RC_SVCNAME}.pid"
output_log="/home/mare/Baram/logs/api.log"
error_log="/home/mare/Baram/logs/api-error.log"

depend() {
    need net
    after docker baram-embedding
}

start_pre() {
    checkpath -d -o mare:mare /home/mare/Baram/logs
}
INITD

chmod +x /etc/init.d/baram-api
echo "  -> /etc/init.d/baram-api created"

# --- 3. Enable services ---
echo "[3/4] Enabling services..."

rc-update add baram-embedding default 2>/dev/null || true
rc-update add baram-api default 2>/dev/null || true

echo "  -> Services added to default runlevel"
echo "  -> Start with: service baram-embedding start"
echo "  -> Start with: service baram-api start"

# --- 4. Set up cron jobs ---
echo "[4/4] Setting up cron jobs for ${USERNAME}..."

# Ensure log directory exists
su - "$USERNAME" -c "mkdir -p ${BARAM_DIR}/logs"

# Write crontab for the user
CRON_TMP=$(mktemp)
cat > "$CRON_TMP" << CRON
# Baram automated tasks
# Installed by scripts/alpine-migration/03-services.sh

# Environment
SHELL=/bin/bash
PATH=/home/${USERNAME}/.cargo/bin:/usr/local/bin:/usr/bin:/bin
HOME=/home/${USERNAME}

# Crawl all categories every 30 minutes
*/30 * * * * ${BARAM_DIR}/scripts/crawl-only.sh

# Index new articles every 2 hours
0 */2 * * * ${BARAM_DIR}/scripts/index-only.sh

# Rotate logs weekly (keep 4 weeks)
0 3 * * 0 find ${BARAM_DIR}/logs -name "*.log" -mtime +28 -delete
CRON

crontab -u "$USERNAME" "$CRON_TMP"
rm -f "$CRON_TMP"

echo "  -> Crontab installed for ${USERNAME}:"
crontab -l -u "$USERNAME"

# --- Verify dcron is running ---
if ! service dcron status &>/dev/null; then
    echo ""
    echo "  Starting dcron..."
    rc-update add dcron default 2>/dev/null || true
    service dcron start
fi

# --- Summary ---
echo ""
echo "============================================="
echo " Services Setup Complete!"
echo "============================================="
echo ""
echo " OpenRC services:"
echo "   baram-embedding  - Embedding server (:8090)"
echo "   baram-api        - REST API server (:8080)"
echo ""
echo " Cron jobs (${USERNAME}):"
echo "   */30 * * * *  crawl-only.sh    (every 30 min)"
echo "   0 */2 * * *   index-only.sh    (every 2 hours)"
echo "   0 3 * * 0     log cleanup      (weekly)"
echo ""
echo " Commands:"
echo "   service baram-embedding start|stop|restart|status"
echo "   service baram-api start|stop|restart|status"
echo "   crontab -l -u ${USERNAME}  (view cron)"
echo "============================================="
