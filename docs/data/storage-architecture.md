# Storage Architecture

## Purpose

Define the complete storage architecture of Objective, including all storage subsystems, data placement, backup strategies, retention policies, and snapshot management.

## Scope

This document covers Kuzu DB storage, vector storage (LanceDB), document archive storage, metadata storage, snapshot and backup systems, and storage configuration. It does not cover the data model, which is defined in `knowledge-graph.md`.

## Responsibilities

- Define storage subsystem boundaries and responsibilities
- Specify data placement and directory layout
- Define backup and snapshot procedures
- Document retention policies
- Define storage configuration
- Specify recovery procedures

## Assumptions

- All storage is on local filesystem
- Storage subsystems are embedded (no external servers)
- Total data volume is manageable on consumer hardware (<100GB typical)
- Filesystem supports atomic renames (ext4, APFS, NTFS)
- Backups may use external storage (user-configured)

## Design

### Storage Subsystems

```
┌─────────────────────────────────────────────────────────────────┐
│                    OBJECTIVE STORAGE                             │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐   │
│  │  Knowledge    │  │   Vector     │  │   Document Archive    │   │
│  │  Graph        │  │   Store      │  │                      │   │
│  │  (Kuzu DB)    │  │  (LanceDB)   │  │  (Compressed JSON,   │   │
│  │               │  │              │  │   partition by month) │   │
│  └──────────────┘  └──────────────┘  └──────────────────────┘   │
│                                                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐   │
│  │  Audio        │  │  Config      │  │   Queue Store         │   │
│  │  Archive      │  │  & State     │  │   (NATS JetStream)    │   │
│  │  (MP3/OGG,    │  │  (YAML/JSON) │  │                      │   │
│  │   dated dirs) │  │              │  │                      │   │
│  └──────────────┘  └──────────────┘  └──────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                 Snapshot / Backup Layer                    │   │
│  │        (snapshot.timestamp/, cold/, hot/)                  │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Directory Layout

```
~/.objective/                              # Default data root
├── db/                                    # Knowledge graph (Kuzu)
│   ├── database/                          # Kuzu database files
│   │   ├── catalog.h                      # Schema catalog
│   │   ├── wal/                           # Write-ahead log
│   │   └── data/                          # Columnar data files
│   └── journal/                           # Transaction journal
│
├── vectors/                               # Vector store (LanceDB)
│   ├── embeddings.lance/                  # Lance dataset
│   └── index/                             # Vector index files
│
├── documents/                             # Document archive
│   ├── 2026/                              # Year partition
│   │   ├── 01/                            # Month partition
│   │   │   ├── <uuid>.json.gz             # Compressed document
│   │   │   └── index.db                   # Per-partition index
│   │   └── ...
│   └── archive/                           # Old partitions (retention)
│
├── audio/                                 # Audio archive
│   ├── broadcasts/                        # Generated broadcasts
│   │   ├── 2026-06-02/                    # Date partition
│   │   │   ├── morning-brief.mp3
│   │   │   └── breaking-news-123.mp3
│   │   └── ...
│   └── library/                           # Permanent audio assets
│       ├── intro.mp3
│       ├── outro.mp3
│       └── transitions/
│
├── config/                                # Configuration
│   ├── objective.yaml                     # Main config file
│   ├── sources.yaml                       # Source definitions
│   ├── models.yaml                        # Model registry
│   └── users.yaml                         # User preferences
│
├── state/                                 # Internal state
│   ├── scheduler.jobstate                 # Scheduler state
│   ├── ingestion.cursors                  # Ingestion cursors (for incremental fetch)
│   └── plugin.registry                    # Plugin state
│
├── queue/                                 # NATS JetStream storage
│   └── jetstream/                         # Stream data
│
├── logs/                                  # Log files
│   ├── objective.log                      # Main log
│   ├── ingestion.log
│   ├── extraction.log
│   ├── correlation.log
│   └── broadcast.log
│
├── models/                                # Downloaded models
│   ├── llama/                             # LLM model files
│   │   ├── mistral-7b-instruct-v0.3.Q4_K_M.gguf
│   │   └── mixtral-8x7b-instruct.Q4_K_M.gguf
│   └── embeddings/                        # Embedding model files
│       └── bge-small-en-v1.5/
│
└── snapshots/                             # System snapshots
    ├── hot/                               # Hot (frequent) snapshots
    │   ├── 2026-06-02T060000Z/
    │   └── 2026-06-02T120000Z/
    └── cold/                              # Cold (less frequent) snapshots
        ├── 2026-06-01T000000Z/
        └── 2026-05-25T000000Z/
```

### Knowledge Graph Storage (Kuzu DB)

**Configuration:**
```yaml
kuzu:
  database_path: "~/.objective/db/database"
  buffer_pool_size: 4096   # MB — 50% of available RAM recommended
  max_threads: 4            # Query parallelism
  checkpoint_threshold: 1024  # MB — WAL size before auto-checkpoint
  enable_compression: true
  auto_checkpoint: true
```

**Storage characteristics:**
- Columnar storage format optimized for analytical queries
- WAL-based transactional writes with checkpointing
- Compression enabled by default for string columns
- Buffer pool managed by Kuzu; no separate cache layer needed

**Maintenance operations:**
```yaml
maintenance:
  vacuum: "weekly"           # Reclaim space from deleted nodes/edges
  analyze: "daily"           # Update query statistics
  checkpoint_interval: "1h"  # Force checkpoint if no auto-checkpoint
```

### Vector Storage (LanceDB)

**Configuration:**
```yaml
lancedb:
  database_path: "~/.objective/vectors/embeddings.lance"
  embedding_dimension: 384
  index_type: "IVF_PQ"       # IVF with product quantization
  num_partitions: 256
  max_vectors_per_file: 100000
  cache_size: 1000           # Recently accessed vectors
```

**Collections:**
| Collection | Content | Dimensions | Index |
|-----------|---------|-----------|-------|
| `entity_embeddings` | Entity descriptions | 384 | IVF_PQ |
| `claim_embeddings` | Claim text | 384 | IVF_PQ |
| `document_embeddings` | Document title + body | 384 | IVF_PQ |
| `event_embeddings` | Event descriptions | 384 | IVF_PQ |

**Query patterns:**
```python
# Find similar claims to a query
similar = table.search(query_vector).limit(20).to_list()

# Hybrid search (vector + metadata filter)
similar = table.search(query_vector).where("confidence > 0.5").limit(10).to_list()
```

### Document Archive Storage

**Format:**
- Compressed JSON (gzip) — one file per document
- File name: `<uuid>.json.gz`
- Content: RawDocument JSON schema (see `service-boundaries.md`)

**Partitioning:**
- Top-level partition: year
- Sub-partition: month
- On-disk index per partition: UUID → filename mapping

**Retention Policy:**
```yaml
document_retention:
  default_days: 90
  per_source:
    rss: 30
    sec_filing: 365
    podcast_transcript: 180
  archive_after_days: 30  # Move to slow storage after 30 days
  compression: "gzip"
  target_size_mb: 100     # Max partition size before rollover
```

### Backup and Snapshot Strategy

**Snapshot types:**

| Type | Frequency | Contents | Retention |
|------|-----------|----------|-----------|
| Hot | Every 6 hours | Kuzu DB, LanceDB, Scheduler state | 7 days |
| Cold | Daily | Complete data dir | 30 days |
| Archive | Weekly | Cold snapshot to external | 90 days |

**Snapshot contents:**
```
snapshot/
├── db/                     # Kuzu DB directory (copied, not hardlinked)
├── vectors/                # LanceDB directory
├── documents/              # Latest month partition (index only, not content)
├── state/                  # Scheduler state, ingestion cursors
├── config/                 # Configuration files
├── manifest.json           # Snapshot metadata
└── checksums.sha256        # Integrity verification
```

**Backup script (conceptual):**
```bash
#!/bin/bash
# snapshot.sh — Create system snapshot

SNAPSHOT_DIR="$DATA_ROOT/snapshots/hot/$(date -u +%Y-%m-%dT%H%M%SZ)"
mkdir -p "$SNAPSHOT_DIR"

# Perform Kuzu checkpoint
objective checkpoint

# Copy database files (Kuzu is ACID; consistent on checkpoint)
cp -r "$DATA_ROOT/db/database" "$SNAPSHOT_DIR/db"
cp -r "$DATA_ROOT/vectors" "$SNAPSHOT_DIR/vectors"
cp -r "$DATA_ROOT/state" "$SNAPSHOT_DIR/state"
cp -r "$DATA_ROOT/config" "$SNAPSHOT_DIR/config"

# Generate manifest
cat > "$SNAPSHOT_DIR/manifest.json" <<EOF
{
  "timestamp": "$(date -u -Iseconds)",
  "version": "$(objective --version)",
  "components": ["db", "vectors", "state", "config"],
  "checksums": {}
}
EOF

# Generate checksums
find "$SNAPSHOT_DIR" -type f -not -name "checksums.sha256" -exec sha256sum {} \; \
  > "$SNAPSHOT_DIR/checksums.sha256"

echo "Snapshot created: $SNAPSHOT_DIR"
```

**Restore script (conceptual):**
```bash
#!/bin/bash
# restore.sh — Restore from snapshot

SNAPSHOT_PATH="$1"
if [ -z "$SNAPSHOT_PATH" ]; then
    echo "Usage: $0 <snapshot-path>"
    exit 1
fi

# Verify integrity
sha256sum -c "$SNAPSHOT_PATH/checksums.sha256" || exit 1

# Stop objective
objective stop

# Backup current state (just in case)
mv "$DATA_ROOT/db" "$DATA_ROOT/db.bak.$(date +%s)"
mv "$DATA_ROOT/vectors" "$DATA_ROOT/vectors.bak.$(date +%s)"

# Restore from snapshot
cp -r "$SNAPSHOT_PATH/db" "$DATA_ROOT/db"
cp -r "$SNAPSHOT_PATH/vectors" "$DATA_ROOT/vectors"
cp -r "$SNAPSHOT_PATH/state" "$DATA_ROOT/state"
cp -r "$SNAPSHOT_PATH/config" "$DATA_ROOT/config"

# Start objective
objective start
```

### Data Integrity

- All snapshots include SHA-256 checksum manifest
- Kuzu DB uses internal checksums on pages
- Document content_hash (SHA-256) verified on read
- Vector store has its own integrity checks (LanceDB)
- Periodic integrity scan (weekly) verifies all subsystems

### Storage Size Estimates

| Component | Growth Rate | 30 Days | 90 Days | 1 Year |
|-----------|------------|---------|---------|--------|
| Kuzu DB | ~50MB/day | 1.5GB | 4.5GB | 18GB |
| LanceDB | ~20MB/day | 600MB | 1.8GB | 7.2GB |
| Documents | ~100MB/day (compressed) | 3GB | 9GB | 36GB |
| Audio | ~10MB/broadcast | 1.5GB (6/day) | 4.5GB | 18GB |
| Logs | ~10MB/day | 300MB | 900MB | 3.6GB |
| **Total** | | **6.9GB** | **20.7GB** | **82.8GB** |

Estimates assume ~1000 documents/day, 6 broadcasts/day, typical news consumption.

## Interfaces

- `knowledge-graph.md` — data model stored in Kuzu
- `provenance-model.md` — metadata tracking in all storage
- `docs/ingestion/ingestion-architecture.md` — document flow through storage
- `docs/broadcast/audio-system.md` — audio storage and streaming

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Disk full | All writes fail | Monitoring alert at 80%, 90%, 95% |
| Kuzu corruption | Graph unavailable | Restore from latest snapshot |
| Snapshot corruption | Restore fails | Verify checksums before restore; keep multiple snapshots |
| Retention pruning fails | Disk grows unbounded | Exponential retry, alert on failure |
| Document index corruption | Can't find documents | Rebuild index from filesystem scan |
| Concurrent snapshot + write | Inconsistent snapshot | Snapshot triggers checkpoint first |
| Filesystem not supported | Storage failures | Verify filesystem compatibility at install |

## Future Extensions

- S3/Blob storage integration for document archive (cold tier)
- Encrypted storage at rest
- Compression level configuration per subsystem
- Automatic tiering (hot/warm/cold) based on access patterns
- Incremental snapshots for faster backups
- Remote backup targets (S3, rsync, rclone)
- Storage quota enforcement per user
