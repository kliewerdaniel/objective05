# Service Boundaries

## Purpose

Define every service and subsystem in Objective, including their inputs, outputs, responsibilities, failure modes, and interfaces with other services.

## Scope

This document covers all internal services, their precise boundaries, data ownership, and interaction contracts. It serves as the authoritative reference for service decomposition.

## Responsibilities

- Define each service's area of ownership
- Specify inputs and outputs for every service
- Document inter-service contracts
- Identify shared-nothing boundaries
- Define data ownership per service

## Assumptions

- Services communicate exclusively through the message bus
- Each service owns its portion of the knowledge graph (CRUD responsibility)
- Services are stateless where possible (state lives in the knowledge graph)
- A service may be a single binary process or a group of related tasks

## Design

### Service Inventory

```
┌─────────────────────────────────────────────────────────┐
│                   OBJECTIVE SERVICES                      │
├──────────────────────────────────────────────────────────┤
│  1. Scheduler Service                                     │
│  2. Ingestion Service (with Source Adapters)              │
│  3. Source Registry Service                               │
│  4. Document Store Service                                │
│  5. Extraction Service                                    │
│  6. Embedding Service                                     │
│  7. Knowledge Graph Service                               │
│  8. Correlation Service                                   │
│     ├─ Event Engine                                       │
│     ├─ Narrative Engine                                   │
│     └─ Contradiction Engine                               │
│  9. Event Repository Service                              │
│ 10. Broadcast Service                                     │
│     ├─ Report Generator                                   │
│     ├─ Priority Selector                                  │
│     └─ Audio Generator                                    │
│ 11. Model Runtime Service                                 │
│ 12. Monitoring Service                                    │
│ 13. Recovery Service                                      │
│ 14. Health Service                                        │
│ 15. API Gateway                                           │
│ 16. Plugin Host                                           │
└──────────────────────────────────────────────────────────┘
```

---

### 1. Scheduler Service

**Ownership:** Job scheduling and triggering

**Responsibility:**
- Manage cron-like periodic job definitions
- Emit trigger events on schedule
- Ensure exactly-once execution semantics
- Handle missed jobs (system was down) by catching up

**Inputs:**
- Job definitions from configuration file
- Job enable/disable commands from API

**Outputs:**
- `scheduler.job.trigger` events on the message bus

**Configuration:**
```yaml
jobs:
  rss_poll:
    schedule: "0 * * * *"    # every hour
    event: "ingestion.poll.rss"
    max_drift: 300            # catch up if behind by < 5 min
  extraction_batch:
    schedule: "*/5 * * * *"  # every 5 minutes
    event: "extraction.process.pending"
  broadcast_generation:
    schedule: "0 */2 * * *"  # every 2 hours
    event: "broadcast.generate"
  maintenance:
    schedule: "0 3 * * *"    # daily at 3am
    event: "system.maintenance"
  snapshot:
    schedule: "0 4 * * *"    # daily at 4am
    event: "system.snapshot"
```

**Failure Modes:**
- Missed jobs: `max_drift` prevents catch-up of old jobs beyond threshold
- Duplicate triggers: Idempotent consumers deduplicate by event ID
- Scheduler crash: Job state persisted; restarts and catches up

---

### 2. Ingestion Service

**Ownership:** Source connection, polling, document fetching

**Responsibility:**
- Manage source adapter lifecycle (start, poll, stop, error)
- Poll sources according to schedule
- Fetch documents from sources
- Normalize documents to internal format
- Publish raw documents to the document queue
- Materialise adapters on demand from the Source Registry

**Inputs:**
- `scheduler.job.trigger` events with type `ingestion.*`
- Source definitions from the Source Registry
- Manual fetch commands from API (UI-triggered fetch, `POST /api/v1/source-registry/:name/trigger`)

**Outputs:**
- `ingestion.document.received` event per document
- Raw document stored in Document Store Service
- `ingestion.poll.complete` event per poll cycle

**Source Adapters:**
Each source type implements:

```rust
trait SourceAdapter {
    fn name(&self) -> &str;
    fn poll(&self) -> Result<Vec<RawDocument>, SourceError>;
    fn validate(&self) -> Result<(), ConfigError>;
    fn health(&self) -> Result<(), HealthError>;
}
```

**Raw Document Schema:**
```json
{
  "id": "uuid-v7",
  "source_type": "rss",
  "source_name": "cnn_world",
  "url": "https://www.cnn.com/world/article-123",
  "title": "Article Title",
  "body": "Full text content...",
  "published_at": "2026-06-02T11:00:00Z",
  "fetched_at": "2026-06-02T11:00:05Z",
  "metadata": {
    "author": "Author Name",
    "language": "en",
    "content_type": "text/html",
    "size_bytes": 45230
  },
  "raw": "<base64-encoded-original>"
}
```

**Failure Modes:**
- Source unreachable: Skip poll, log error, emit `ingestion.source.error`
- Malformed response: Log, do not publish document emit
- Credential expired: Emit `ingestion.source.auth_failure`, disable source
- Rate limiting: Respect Retry-After headers, backoff
- Timeout: Configurable per-source timeout (default 30s)

---

### 3. Source Registry Service

**Ownership:** Persisted catalog of user-managed ingestion sources

**Responsibility:**
- Persist `SourceDefinition` records (name, type, URL, schedule, enabled) to a JSON file under the data root
- Expose CRUD through the API gateway
- Materialise `Box<dyn SourceAdapter>` instances on demand via `SourceRegistry::spawn_adapter`
- Default-seed `hackernews_front` and `lobsters` on first boot

**Inputs:**
- API requests (`GET/POST/PUT/DELETE /api/v1/source-registry[/:name]`)
- Manual trigger requests (`POST /api/v1/source-registry/:name/trigger`)

**Outputs:**
- `SourceDefinition` records returned to the API
- Materialised `Box<dyn SourceAdapter>` for the Ingestion Service to poll

**Storage:**
- Single JSON file at `.objective/state/sources.json`
- Guarded by `tokio::sync::RwLock` so API CRUD and the pipeline worker can read/write concurrently

**Failure Modes:**
- Disk write failure: Surface `RegistryError::Storage` to the API; the in-memory state remains the source of truth until the next successful flush
- Duplicate `name` on create: Return `RegistryError::AlreadyExists` (HTTP 409)
- Unknown `name` on update/delete/trigger: Return `RegistryError::NotFound` (HTTP 404)
- Invalid `url` for the chosen `source_type`: Returned as 400 Bad Request from the gateway

---

### 4. Document Store Service

**Ownership:** Raw document persistence

**Responsibility:**
- Store raw documents (compressed) for archival
- Retrieve documents by ID
- Delete documents by retention policy
- Clean up expired documents

**Inputs:**
- `ingestion.document.received` events
- Document retrieval requests from Extraction Service
- Retention policy configuration

**Outputs:**
- Document content to Extraction Service (on-demand)
- `document.stored` acknowledgment
- `document.deleted` events for cleanup tracking

**Storage:**
- Documents stored as compressed JSON blobs on filesystem
- Indexed by ID, source, date, content type
- Partitioned by month for efficient pruning
- Configurable retention (default 90 days)

**Failure Modes:**
- Disk full: Stop ingestion, alert, resume when space available
- Corruption on read: Skip document, log, emit `document.corrupt`
- Retention pruning fails: Exponential backoff retry

---

### 5. Extraction Service

**Ownership:** Entity, claim, and relationship extraction from raw documents

**Responsibility:**
- Receive raw documents from ingestion
- Run documents through extraction pipeline
- Extract entities (people, organizations, locations, topics, concepts)
- Extract claims (factual statements attributed to sources)
- Extract relationships (connections between entities)
- Deduplicate against existing knowledge graph
- Merge new extractions into knowledge graph
- Emit extraction events

**Inputs:**
- Raw documents (from Document Store or event payload)
- Existing knowledge graph state (for deduplication)
- Extraction model configuration

**Outputs:**
- `extraction.entity.extracted` event
- `extraction.claim.extracted` event
- `extraction.relationship.formed` event
- Knowledge graph mutations

**Pipeline Stages:**
```
1. Document Chunking
   - Split large documents into manageable chunks (max 4096 tokens)
   - Preserve document structure (headings, paragraphs)
   
2. Entity Extraction
   - Identify named entities with NER model
   - Resolve entity types (Person, Organization, Location, Event, Concept)
   - Extract entity metadata (aliases, descriptions)
   - Confidence score per entity

3. Claim Extraction
   - Identify factual statements
   - Extract claim text, subject, predicate, object
   - Attribute to speaker/source within document
   - Confidence score per claim

4. Relationship Extraction
   - Identify relationships between entities in same document
   - Type relationships (works_for, located_in, part_of, etc.)
   - Extract relationship evidence (supporting text)
   - Confidence score per relationship

5. Deduplication
   - Match entities against existing graph
   - Merge claims with same subject/predicate/object
   - Update confidence based on supporting evidence count
   
6. Graph Write
   - Insert/update entities, claims, relationships
   - Link all extractions to source document
   - Write provenance metadata
```

**Failure Modes:**
- Model inference timeout: Retry with smaller chunk size
- Extraction returns empty: Log, accept as valid (no entities found)
- Deduplication conflict: Timestamp-priority merge (see ADR-007)
- Graph write conflict: Retry with backoff

---

### 6. Embedding Service

**Ownership:** Text embedding generation

**Responsibility:**
- Generate embeddings for entities, claims, documents
- Manage embedding cache (avoid re-embedding)
- Store embeddings in LanceDB
- Provide similarity search API

**Inputs:**
- Text content (entities, claims, documents) from Extraction Service
- Query embeddings from Correlation Service

**Outputs:**
- Embedding vectors stored in LanceDB
- Similarity search results

**Configuration:**
```yaml
model:
  type: "bge-small-en-v1.5"
  dimension: 384
  batch_size: 32
  cache_size: 10000
```

**Failure Modes:**
- Embedding model fails: Fall back to sparse (BM25) search
- Cache miss ratio high: Performance degrades, alert for model change
- Vector store full: LRU eviction policy

---

### 7. Knowledge Graph Service

**Ownership:** Kuzu DB lifecycle, graph operations, query API

**Responsibility:**
- Manage Kuzu DB connection pool
- Execute graph queries on behalf of other services
- Enforce schema constraints
- Manage graph transactions
- Provide query result serialization
- Manage graph snapshots and restores

**Inputs:**
- Graph mutation commands (CREATE, MERGE, DELETE)
- Graph query requests (MATCH, temporal queries)
- Snapshot/restore commands

**Outputs:**
- Query results (entities, claims, events, narratives, contradictions)
- Graph mutation acknowledgments
- Snapshot files on disk

**Schema Enforcement:**
- All graph writes validated against defined schema
- Schema version tracked in graph metadata
- Schema migrations executed at startup

**Failure Modes:**
- Kuzu crash: Connection pool retry, process restart
- Schema violation: Reject write, log error, return error to caller
- Transaction timeout: Rollback, retry
- Database corruption: Restore from latest snapshot

---

### 8. Correlation Service

**Ownership:** Event formation, narrative detection, contradiction detection

**Sub-services:**

#### 8a. Event Engine

**Responsibility:**
- Group related claims into events
- Score event confidence based on supporting evidence
- Merge duplicate events
- Track event evolution over time
- Detect event completion (no new claims expected)

**Inputs:** New claims, existing events, temporal context

**Outputs:** Event mutations, `correlation.event.*` events

#### 8b. Narrative Engine

**Responsibility:**
- Cluster related events into narratives
- Score narrative strength (coverage, diversity, recency)
- Track narrative evolution over time
- Detect narrative branching (forks, splits)
- Rank narratives by significance

**Inputs:** Events, relationships, temporal context

**Outputs:** Narrative mutations, `correlation.narrative.*` events

#### 8c. Contradiction Engine

**Responsibility:**
- Detect contradictory claims about the same subject
- Classify contradiction types (direct, implied, temporal)
- Score contradiction confidence
- Suggest resolution strategies
- Track resolved contradictions

**Inputs:** Claims, events, entities, temporal context

**Outputs:** Contradiction records, `correlation.contradiction.*` events

---

### 9. Event Repository Service

**Ownership:** Persistence of derived `Event` records (the correlation engine's output)

**Responsibility:**
- Persist `Event` records produced by the Event Engine
- Provide lookup by id and bulk listing for the API
- Replace the full event set during maintenance / status transitions
- Survive daemon restarts (file-backed implementation)

**Interface:**
```rust
#[async_trait]
trait EventRepository: Send + Sync {
    async fn save_event(&self, event: Event) -> Result<()>;
    async fn list_events(&self) -> Result<Vec<Event>>;
    async fn get_event(&self, id: &str) -> Result<Option<Event>>;
    async fn replace_all(&self, events: Vec<Event>) -> Result<()>;
}
```

**Implementations:**
- `InMemoryEventRepository` — used by the runtime test suite and as a fast in-process cache
- `FileEventRepository` — production runtime; writes the full event set to a JSON file at `.objective/state/events.json` after every mutation using a write-to-temp-then-rename atomic flush

**Inputs:** `Event` records from the Event Engine, status updates from the API (`POST /api/v1/events/:id/resolve`)

**Outputs:** `Event` records returned to the API, persistence to disk

**Failure Modes:**
- Disk write failure: Surface as `ObjectiveError::Storage`; the in-memory copy is preserved until the next successful flush
- JSON parse failure on load: Treat as empty store, log a warning, continue (operator can inspect or delete the corrupted file)

---

### 10. Broadcast Service

**Ownership:** Report generation, audio generation, broadcast scheduling

**Sub-services:**

#### 10a. Report Generator

**Responsibility:**
- Query knowledge graph for broadcast-relevant content
- Generate written reports in natural language
- Format reports for different consumption modes (brief, full, audio script)
- Rank content by significance

**Inputs:** Graph queries, broadcast configuration, user interests

**Outputs:** Report text, `broadcast.report.generated` event

#### 10b. Priority Selector

**Responsibility:**
- Rank all pending content by priority
- Balance breaking news vs. regular content
- Ensure coverage diversity (topics, sources, perspectives)
- Respect user interest profiles

**Inputs:** All pending reports, user interests, breaking event signals

**Outputs:** Prioritized broadcast queue

#### 10c. Audio Generator

**Responsibility:**
- Convert report text to speech
- Manage voice pools
- Generate podcast-style audio (multi-voice, transitions)
- Archive generated audio
- Expose streaming endpoint

**Inputs:** Report text, voice configuration, audio format

**Outputs:** Audio files, `broadcast.audio.ready` event

---

### 11. Model Runtime Service

**Ownership:** LLM and embedding model lifecycle

**Responsibility:**
- Load and unload models
- Manage model context windows
- Execute inference requests
- Manage model queues (avoid OOM from concurrent requests)
- Cache model responses where safe
- Report model health and performance metrics

**Inputs:** Inference requests (prompt, model_id, parameters)

**Outputs:** Inference responses (text, logprobs, usage metrics)

**Model Registry:**
```yaml
models:
  extraction:
    type: "llama.cpp"
    path: "/var/lib/objective/models/mistral-7b-instruct-v0.3.Q4_K_M.gguf"
    context: 8192
    gpu_layers: 32
    
  embedding:
    type: "onnx"
    path: "/var/lib/objective/models/bge-small-en-v1.5"
    dimension: 384
    
  report:
    type: "llama.cpp"
    path: "/var/lib/objective/models/mixtral-8x7b-instruct.Q4_K_M.gguf"
    context: 32768
    gpu_layers: 32
```

---

### 12. Monitoring Service

**Ownership:** Per-pipeline counters and uptime tracking

**Responsibility:**
- Record pipeline activity (`document_ingested`, `extraction_completed`, `event_detected`, `pipeline_cycle`, `retry`, `dead_letter`, `snapshot_created`, `error`)
- Track daemon uptime and last activity timestamp
- Persist the metric snapshot to `.objective/state/monitoring.json` so counters survive a restart
- Expose counters via `GET /api/v1/monitoring`

**Inputs:** Counter updates from the pipeline worker, scheduler, retry queue, and snapshot service

**Outputs:** `PipelineMetrics` JSON, persisted snapshot

**Failure Modes:**
- Persist failure: Counters stay accurate in memory; the on-disk snapshot may lag by one write

---

### 13. Recovery Service

**Ownership:** Pipeline stall detection and recovery publishing

**Responsibility:**
- Poll the Monitoring Service on a configurable cadence
- Compare the current metric snapshot against the previous one to detect a stalled pipeline (no activity, error count climbing)
- Publish `system.service.crash` when both `pipeline_cycles` stopped advancing and `errors` grew within the window
- Publish `system.service.recovered` when the pipeline returns to a healthy state
- Publish periodic `system.heartbeat` events on a separate cadence
- Persist recovery history (capped) and the last-checked timestamp to `.objective/state/recovery.json` so an operator can audit past incidents
- Expose state via `GET /api/v1/recovery` and force a check via `POST /api/v1/recovery/check`

**Inputs:** `PipelineMetrics` snapshots from the Monitoring Service, API force-check requests

**Outputs:** `system.heartbeat`, `system.service.crash`, `system.service.recovered` events on the message bus; recovery JSON

**Failure Modes:**
- Monitoring Service unreachable: Skip this tick, do not flip to `Crash`; the watchdog itself never publishes a crash based on its own outage

---

### 14. Health Service

**Ownership:** System monitoring and self-healing

**Responsibility:**
- Aggregate health metrics from all services
- Detect service failures (heartbeat timeout, error rate spike)
- Execute recovery procedures (restart, circuit break)
- Expose health endpoint for external monitoring
- Maintain system state log
- Emit system events (crash, recovery, degradation)

---

### 15. API Gateway

**Ownership:** External API, Web UI, client communication

**Responsibility:**
- Expose REST and WebSocket APIs
- Authenticate and authorize UI clients
- Proxy dashboard queries to appropriate services
- Stream real-time events to WebSocket clients
- Serve dashboard static assets
- Expose operator-managed surfaces for the Source Registry and Plugin Host
  - `GET/POST /api/v1/source-registry`, `GET/PUT/DELETE /api/v1/source-registry/:name`, `POST /api/v1/source-registry/:name/trigger`
  - `GET /api/v1/plugins`, `GET /api/v1/plugins/:name`, `POST /api/v1/plugins/:name/restart`, `POST /api/v1/plugins/reload`
- Expose the Monitoring and Recovery services
  - `GET /api/v1/monitoring`
  - `GET /api/v1/recovery`, `POST /api/v1/recovery/check`
- Surface the live configuration via `GET /api/v1/config` (503 when not attached)
- Bridge the in-process `MessageBus` to a `/ws` WebSocket endpoint with per-connection channel filtering (`events`, `broadcast`, `system`) and a documented welcome / event / pong / error wire protocol
- Publish an OpenAPI schema at `GET /api-docs/openapi.json` (utoipa-annotated handlers)

---

### 16. Plugin Host

**Ownership:** Plugin lifecycle management and event routing to plugins

**Responsibility:**
- Discover plugin manifests under `.objective/plugins/<name>/plugin.json`
- Register built-in plugins (in-process `Arc<dyn Plugin>` objects) at startup
- Validate, start, and stop each registered plugin (`on_start` / `on_stop`)
- Route bus events to plugins whose manifest subscription filters match (`event_types`, optional `entity_types`)
- Call `check_health` on a configurable cadence
- Track lifecycle state per plugin (`Discovered`, `Validated`, `Running`, `Error`)
- Persist plugin state to `.objective/state/plugins.json` and individual `PluginOutput::State` key/value entries to the same file
- Surface plugin status via `GET /api/v1/plugins`, `GET /api/v1/plugins/:name`, `POST /api/v1/plugins/:name/restart`, `POST /api/v1/plugins/reload`
- Increment a per-plugin `consecutive_failures` counter on handler error or timeout (handler is wrapped in a tokio timeout) and a `restart_count` on each forced restart

**Plugin Trait:**
```rust
#[async_trait]
trait Plugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    async fn on_start(&self) -> Result<(), PluginError>;
    async fn on_stop(&self) -> Result<(), PluginError>;
    async fn handle(&self, event: &EventEnvelope) -> Result<Vec<PluginOutput>, PluginError>;
    async fn check_health(&self) -> PluginHealth;
}
```

**v1 Scope (in-process only):**
- Built-in plugins are `Arc<dyn Plugin>` objects registered at startup
- Discovered manifests are mounted as `NoopPlugin` placeholders so they show up in the API and counts; their `event_types` and `entity_types` are honoured for routing
- The gRPC contract from `docs/api/plugin-api.md` is the future upgrade path; the API surface and lifecycle states mirror the spec so the swap is mechanical

**Built-ins Shipped Today:**
- `audit-log` — records every event the host routes to it; outputs `PluginOutput::Log` lines (error / warn / info)
- `re-emitter` — subscribes to `extraction.document.processed` and republishes the event on `plugin.re_emitted`

**Inputs:** Bus events (via the in-process `MessageBus`), `register_builtin` and `discover_into` calls from `objective` startup, API force-restart / reload requests

**Outputs:** `PluginOutput::Log` lines, `PluginOutput::Publish` envelopes pushed back onto the bus, `PluginOutput::State` key/value updates persisted to disk, plugin status snapshots to the API

**Failure Modes:**
- Handler error: `consecutive_failures` incremented, `last_error` set, `state` flips to `Error`; next health tick auto-flips back to `Running` once failures reset to zero
- Handler timeout (configurable, default 5 s): same handling as an error with `plugin handler timeout` as `last_error`
- Unknown plugin on restart / get: `HostError::UnknownPlugin` (HTTP 404)
- Duplicate registration: `HostError::DuplicatePlugin` (HTTP 409)
- State file read/write failure: `HostError::State` surfaced to the API; in-memory state remains authoritative until the next successful flush

---

## Interfaces Summary

| Service | Subscribes To | Publishes To | Reads From | Writes To |
|---------|--------------|-------------|-----------|----------|
| Scheduler | API (config) | `scheduler.job.trigger` | Config | Job state (`.objective/state/scheduler.jobstate`) |
| Ingestion | `scheduler.job.trigger` | `ingestion.document.*` | Source Registry, Config | Document Store |
| Source Registry | API CRUD | `SourceDefinition` snapshots | `.objective/state/sources.json` | `.objective/state/sources.json` |
| Document Store | `ingestion.document.received` | `document.stored` | Filesystem | Filesystem |
| Extraction | `ingestion.document.received` | `extraction.*` | Document Store, Graph | Graph |
| Embedding | Extraction events | Embedding vectors | Text content | LanceDB |
| Knowledge Graph | All mutation events | Query results | Kuzu DB | Kuzu DB |
| Correlation | `extraction.*` | `correlation.*` | Graph | Graph, Event Repository |
| Event Repository | Save/replace calls from Event Engine, API | Event records | `.objective/state/events.json` | `.objective/state/events.json` |
| Broadcast | `correlation.*`, Scheduler | `broadcast.*` | Graph | Graph, Audio store |
| Model Runtime | Inference requests | Inference results | Model files | None |
| Monitoring | Pipeline counter updates | `PipelineMetrics` JSON | Pipeline worker, Scheduler, Retry queue, Snapshot | `.objective/state/monitoring.json` |
| Recovery | `PipelineMetrics` from Monitoring | `system.heartbeat`, `system.service.crash`, `system.service.recovered` | `.objective/state/recovery.json` | `.objective/state/recovery.json` |
| Health | All public events | `system.*` aggregate | All service probes | Metrics store |
| API Gateway | All public events | WebSocket push | All services | None |
| Plugin Host | All bus events, filters by subscription | `PluginOutput::Publish` envelopes | `.objective/plugins/<name>/plugin.json`, `.objective/state/plugins.json` | `.objective/state/plugins.json` |

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Service boundary violation | Coupling, testing difficulty | Linter enforces inter-service communication goes through bus |
| Blurred data ownership | Write conflicts, inconsistency | Each data entity has a single owning service |
| Missing input contract | Integration failures | All inputs validated against schema on receipt |
| Service too large | Maintenance burden | Split criteria: >2000 LOC, >5 responsibilities, >3 data stores touched |

## Future Extensions

- Service sharding for horizontal scaling
- Service mesh for multi-machine deployment
- Dynamic service registration and discovery
- Circuit breaker patterns between all service pairs
- Rate limiting per service for backpressure
