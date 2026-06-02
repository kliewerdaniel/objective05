# Operations

## Purpose

Document the operational procedures for running, updating, monitoring, backing up, and recovering Objective in production use.

## Scope

This document covers daily operations, startup/shutdown procedures, update procedures, backup and restore, recovery from failures, and data management tasks.

## Responsibilities

- Define standard operating procedures
- Specify startup and shutdown sequences
- Document update procedures
- Define backup and restore procedures
- Document disaster recovery
- Specify data management tasks

## Assumptions

- Objective runs as a daemon/service (long-running process)
- System has been installed according to `installation.md`
- Users may want Objective to start automatically on boot
- Regular maintenance is required for healthy operation

## Design

### Running Objective

**Start:**

```bash
# Start Objective (foreground)
objective start

# Start Objective (background daemon)
objective start --daemon

# Start Objective with specific data directory
objective start --data-dir /mnt/external-drive/objective-data

# Start with verbose logging
objective start --verbose
```

**Stop:**

```bash
# Graceful stop — waits for in-progress operations to complete (max 30s)
objective stop

# Force stop
objective stop --force

# Restart
objective restart
```

**Status:**

```bash
# System status
objective status

# Status output:
# Objective 1.0.0
# Uptime: 3d 14h 22m
# Services:
#   ingestion:   healthy (last poll: 2m ago)
#   extraction:  healthy (queue: 12 pending)
#   correlation: healthy
#   broadcast:   healthy (next: 14:00)
#   model:       healthy (models loaded: 3/3)
# Storage: 8.2 GB / 100 GB (8%)
# Sources: 5 active, 0 errors
```

**Auto-start:**

```bash
# Register as systemd service (Linux)
sudo objective setup --systemd

# Register as launchd service (macOS)
objective setup --launchd

# Register as Windows service
objective setup --windows-service
```

**Systemd unit file (Linux):**
```ini
[Unit]
Description=Objective Intelligence System
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/objective start --daemon
Restart=on-failure
RestartSec=10
User=objective
Group=objective

[Install]
WantedBy=multi-user.target
```

### Updating Objective

```bash
# Check for update
objective update --check
# Output: Update available: 1.1.0 (current: 1.0.0)

# Download update
objective update --download

# Apply update (requires restart)
objective update --apply
objective restart

# Automatic update (check + download + apply)
objective update
```

**Update process:**
1. Check current version against update server
2. Download new binary to temporary location
3. Verify binary signature (if available)
4. Create backup of current state (automatic)
5. Stop current process gracefully
6. Replace binary with new version
7. Run database migrations (if needed)
8. Start new process
9. Verify health after start
10. If start fails: roll back to previous binary

**Rollback:**

```bash
# Roll back to previous version
objective update --rollback
objective restart
```

### Backups

**Automatic snapshot schedule:**

| Snapshot | Frequency | Retention | Contents |
|----------|-----------|-----------|----------|
| Hot | Every 6 hours | 7 days | DB, vectors, state |
| Cold | Daily | 30 days | Complete data |
| Archive | Weekly | 90 days | Compressed + external |

**Manual backup:**

```bash
# Create snapshot now
objective snapshot create

# Create snapshot with custom name
objective snapshot create --name "before-update-v1.1.0"

# List snapshots
objective snapshot list

# Restore from snapshot
objective snapshot restore --name "before-update-v1.1.0"
```

**External backup configuration:**

```yaml
backup:
  external:
    enabled: false
    target: "s3://my-bucket/objective-backups"
    type: "s3"               # s3, rclone, rsync, local
    schedule: "daily"
    encryption_key: "..."    # Encrypted at rest
    compression: "zstd"
```

**Backup contents:**
```
snapshot-2026-06-02T060000Z/
├── db/                     # Kuzu database
├── vectors/               # LanceDB vectors
├── config/                # Configuration
├── state/                 # Internal state
├── manifest.json          # Metadata
└── checksums.sha256       # Integrity verification
```

### Recovery

**From snapshot:**

```bash
# List available snapshots
objective snapshot list

# Restore from most recent snapshot
objective snapshot restore --latest

# Restore from named snapshot
objective snapshot restore --name "2026-06-01T000000Z"
```

**Recovery scenarios:**

| Scenario | Procedure | Data Loss |
|----------|-----------|-----------|
| Application crash | `objective restart` | None (durable queues) |
| System crash | `objective start` | None (durable queues) |
| Corrupt database | Restore from hot snapshot | Up to 6 hours |
| Disk failure | Restore from cold snapshot + reinstall | Up to 24 hours |
| Accidental data deletion | Restore from pre-deletion snapshot | Up to 6 hours |
| Failed update | Rollback via `objective update --rollback` | None |
| Full disaster (no backup) | Clean install; no data recovery | Complete |

**Specific recovery procedures:**

**Corrupt knowledge graph:**
```bash
# 1. Stop Objective
objective stop

# 2. Run database integrity check
objective db check

# 3. If corrupted, restore from snapshot
objective snapshot restore --latest

# 4. Start Objective
objective start
```

**Failed model load:**
```bash
# 1. Check model integrity
objective models verify

# 2. Redownload corrupt model
objective models download mistral-7b

# 3. Reload models
objective models reload
```

**Ingestion backlog clearing:**
```bash
# Check queue depth
objective queue depth

# Clear specific source queue
objective queue clear --source "rss_reuters"

# Reset all queues (caution)
objective queue reset
```

### Data Management

**Storage optimization:**
```bash
# Check storage usage
objective storage usage

# Vacuum database (reclaim space)
objective db vacuum

# Prune old documents
objective storage prune --older-than 90d

# Archive snapshots
objective snapshot archive --older-than 7d
```

**Data export:**
```bash
# Export all entities
objective export entities --format json -o entities.json

# Export events in date range
objective export events --from 2026-01-01 --to 2026-06-01 --format csv -o events.csv

# Export full graph
objective export graph --format rdf -o graph.ttl

# Export raw documents
objective export documents --format jsonl -o documents.jsonl
```

**Data import:**
```bash
# Import entities from JSON
objective import entities -f entities.json

# Import documents from JSONL
objective import documents -f documents.jsonl
```

### Monitoring

**Health check endpoint:**
```bash
curl http://localhost:8080/api/v1/health
# Response:
# {
#   "status": "healthy",
#   "timestamp": "2026-06-02T12:00:00Z",
#   "uptime_seconds": 123456,
#   "services": {
#     "ingestion": {"status": "healthy", "last_poll": "2026-06-02T11:58:00Z"},
#     "extraction": {"status": "healthy", "queue_depth": 5},
#     "correlation": {"status": "healthy"},
#     "broadcast": {"status": "healthy", "next_broadcast": "2026-06-02T14:00:00Z"},
#     "model_runtime": {"status": "healthy", "models_loaded": 3}
#   },
#   "storage": {
#     "total_gb": 100,
#     "used_gb": 8.2,
#     "available_gb": 91.8
#   }
# }
```

**Log files:**
```
~/.objective/logs/
├── objective.log           # Main log (all services)
├── ingestion.log           # Ingestion-specific
├── extraction.log          # Extraction-specific
├── correlation.log         # Correlation-specific
├── broadcast.log           # Broadcast-specific
├── system.log              # System events
└── audit.log               # Audit trail (append-only)
```

Log rotation: daily, 30 days retention, gzip compressed.

## Interfaces

- `installation.md` — installation prerequisites
- `observability.md` — metrics, logging, tracing
- `docs/architecture/architecture-overview.md` — system architecture for recovery context

## Failure Modes

| Failure | Impact | Mitigation |
|--------|--------|------------|
| Backup disk fills | Backup fails | Monitor backup target; alert at 80% |
| Snapshot corruption | Cannot restore | Verify checksums; keep multiple generations |
| Update fails mid-process | System in unknown state | Atomic binary swap; rollback capability |
| Auto-start fails on boot | System not running | Process monitor (systemd/launchd) restarts on failure |
| Logs fill disk | System degradation | Log rotation with size limit; compression |
| Long-running operation prevents graceful stop | Stop timeout | Force stop after timeout; recovery on restart |

## Future Extensions

- Web-based admin console for operations
- Automated health report email
- Prometheus/Grafana integration for production deployments
- Zero-downtime updates (hot-swap binary)
- Canary updates with automatic rollback
- Maintenance window scheduling
- Multi-instance fleet management
