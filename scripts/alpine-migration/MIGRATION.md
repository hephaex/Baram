# Baram → Proxmox Alpine VM Migration

## Quick Reference

### Execution Order

```
[Proxmox Host]  → Step 1: Create VM
[Alpine VM]     → Step 2: Run 01-bootstrap.sh (as root)
[Alpine VM]     → Step 3: Clone + build (as mare)
[Current Server]→ Step 4: Run 02-migrate-data.sh (rsync to Alpine)
[Alpine VM]     → Step 5: Docker services up
[Alpine VM]     → Step 6: Run 03-services.sh (as root)
[Alpine VM]     → Step 7: Run 04-verify.sh
```

### Step 1: Proxmox VM

```bash
# On Proxmox host
qm create 200 \
  --name baram-alpine \
  --memory 16384 \
  --cores 8 \
  --cpu host \
  --scsihw virtio-scsi-pci \
  --scsi0 local-lvm:200,format=raw \
  --net0 virtio,bridge=vmbr0 \
  --ostype l26 \
  --cdrom local:iso/alpine-virt-3.21.0-x86_64.iso

# Boot → setup-alpine (sys mode) → reboot → remove ISO
```

### Step 2: Bootstrap (on Alpine VM)

```bash
# Transfer and run as root
scp scripts/alpine-migration/01-bootstrap.sh root@ALPINE_IP:/tmp/
ssh root@ALPINE_IP bash /tmp/01-bootstrap.sh
```

### Step 3: Clone + Build (on Alpine VM)

```bash
su - mare
git clone https://github.com/hephaex/baram.git ~/Baram
cd ~/Baram
cargo build --release    # ~4-5 min
cargo test
cargo clippy
```

### Step 4: Data Migration (from current server)

```bash
ALPINE_IP=192.168.x.x bash scripts/alpine-migration/02-migrate-data.sh
```

### Step 5: Docker Services (on Alpine VM)

```bash
cd ~/Baram/docker
cp .env.example .env     # Edit passwords!
docker compose -f docker-compose.yml -f docker-compose.alpine.yml up -d
docker compose ps        # Verify health

# Restore PostgreSQL (if dumped)
docker exec -i baram-postgres psql -U baram baram < /tmp/baram_pg_dump.sql

# Rebuild OpenSearch index
cd ~/Baram
./target/release/baram index --input ./output/raw --force --batch-size 50
```

### Step 6: Services + Cron (on Alpine VM)

```bash
sudo bash scripts/alpine-migration/03-services.sh
```

### Step 7: Verify

```bash
bash scripts/alpine-migration/04-verify.sh
```

## Troubleshooting

| Problem | Solution |
|---------|----------|
| OpenSSL link error | `export OPENSSL_DIR=/usr` |
| candle build fail | `RUSTFLAGS="-C target-feature=-crt-static"` |
| tokenizers cmake error | `apk add protobuf-dev` (included in bootstrap) |
| `date -d` not working | `apk add coreutils` (included in bootstrap) |
| flock not found | `apk add util-linux` (included in bootstrap) |
| Docker socket permission | `addgroup mare docker` + re-login |

## Memory Layout (16GB)

```
docker-compose.alpine.yml overrides:
  PostgreSQL:   2GB (shared_buffers=128MB)
  OpenSearch:   4GB (JVM heap=512MB)
  Redis:        512MB
  ─────────────────
  Docker total: ~6.5GB
  System+Rust:  ~9.5GB available
```
