# Internal API

## Purpose

Define Objective's internal service APIs, event contracts, message formats, and communication patterns used for inter-service communication.

## Scope

This document covers the message bus topics, event schemas, service-to-service communication patterns, REST endpoints exposed by the API gateway, and WebSocket event streams.

## Responsibilities

- Define all internal service APIs
- Specify event contracts and schemas
- Define message formats for all communication
- Document service-to-service interaction patterns
- Specify the external API gateway contract

## Assumptions

- Internal services communicate via NATS message bus (see ADR-003)
- External clients communicate via the API Gateway (REST + WebSocket)
- All events follow a common envelope format
- Services are co-located in the same process (single binary) for MVP
- Schema evolution follows backward-compatible patterns

## Design

### Message Bus Architecture

```
┌─────────────────────────────────────────────┐
│              NATS JetStream                  │
│                                              │
│  Streams:                                    │
│  ┌──────────────────────────────────────┐   │
│  │ ingestion (retention: 7d)            │   │
│  │ extraction (retention: 7d)           │   │
│  │ correlation (retention: 7d)          │   │
│  │ broadcast (retention: 7d)           │   │
│  │ system (retention: 30d)             │   │
│  └──────────────────────────────────────┘   │
│                                              │
│  Subjects:                                   │
│  ingestion.>                                 │
│  extraction.>                                │
│  correlation.>                               │
│  broadcast.>                                 │
│  system.>                                    │
│  scheduler.>                                 │
└─────────────────────────────────────────────┘
```

### Event Envelope

All events follow a standard envelope:

```json
{
  "id": "01J2Y3Z4A5B6C7D8E9F0G1H2I3",
  "type": "ingestion.document.received",
  "version": 1,
  "timestamp": "2026-06-02T12:00:00.000Z",
  "source": "ingestion.rss.reuters_world",
  "correlation_id": "01J2Y3Z4A5B6C7D8E9F0G1H2I3",
  "causation_id": "01J2Y3Z4A5B6C7D8E9F0G1H2I3",
  "data": {},
  "metadata": {
    "producer": "ingestion-service",
    "producer_version": "1.0.0",
    "retry_count": 0,
    "produced_at": "2026-06-02T12:00:00.000Z"
  }
}
```

**Field semantics:**

| Field | Type | Description |
|-------|------|-------------|
| `id` | UUID v7 | Unique event identifier (time-sortable) |
| `type` | String | Event type in dot notation |
| `version` | Integer | Schema version for backward compatibility |
| `timestamp` | ISO8601 | When the event occurred |
| `source` | String | Component that produced the event |
| `correlation_id` | UUID v7 | Ties related events together (set once per processing chain) |
| `causation_id` | UUID v7 | ID of the event that caused this event |
| `data` | Object | Event-specific payload |
| `metadata` | Object | Producer metadata, retry tracking |

### Event Catalog

#### Ingestion Events

| Event Type | Version | Description | Payload |
|------------|---------|-------------|---------|
| `ingestion.document.received` | 1 | New document fetched from source | `{document_id, source_id, source_type, url, title, published_at}` |
| `ingestion.document.duplicate` | 1 | Document already exists | `{document_id, existing_id, match_type}` |
| `ingestion.document.error` | 1 | Document fetch/parse failed | `{source_id, url, error, error_type}` |
| `ingestion.poll.complete` | 1 | Source poll cycle finished | `{source_id, documents_fetched, duplicates, errors, duration_ms}` |
| `ingestion.source.error` | 1 | Source adapter error | `{source_id, error, error_type, fatal}` |
| `ingestion.source.health` | 1 | Source health status | `{source_id, status, latency_ms}` |

#### Extraction Events

| Event Type | Version | Description | Payload |
|------------|---------|-------------|---------|
| `extraction.document.processing` | 1 | Document extraction started | `{document_id}` |
| `extraction.document.processed` | 1 | Document extraction complete | `{document_id, entities_count, claims_count, relationships_count, duration_ms}` |
| `extraction.entity.extracted` | 1 | Entity extracted | `{entity_id, name, type, confidence, document_id}` |
| `extraction.claim.extracted` | 1 | Claim extracted | `{claim_id, claim_text, subject_id, predicate, confidence, document_id}` |
| `extraction.relationship.formed` | 1 | Relationship formed | `{relationship_id, from_id, to_id, type, confidence}` |
| `extraction.error` | 1 | Extraction failed | `{document_id, error, chunk_index}` |

#### Correlation Events

| Event Type | Version | Description | Payload |
|------------|---------|-------------|---------|
| `correlation.event.created` | 1 | New event formed | `{event_id, title, type, importance, confidence}` |
| `correlation.event.updated` | 1 | Event updated | `{event_id, changed_fields, new_importance, new_confidence}` |
| `correlation.event.merged` | 1 | Events merged | `{primary_id, merged_ids}` |
| `correlation.event.status_changed` | 1 | Event status changed | `{event_id, old_status, new_status}` |
| `correlation.narrative.formed` | 1 | New narrative formed | `{narrative_id, title, event_ids, strength}` |
| `correlation.narrative.updated` | 1 | Narrative updated | `{narrative_id, changed_fields, added_event_ids}` |
| `correlation.narrative.status_changed` | 1 | Narrative status changed | `{narrative_id, old_status, new_status}` |
| `correlation.contradiction.detected` | 1 | Contradiction detected | `{contradiction_id, type, severity, confidence, claim_ids}` |
| `correlation.contradiction.resolved` | 1 | Contradiction resolved | `{contradiction_id, resolution_strategy}` |

#### Broadcast Events

| Event Type | Version | Description | Payload |
|------------|---------|-------------|---------|
| `broadcast.scheduled` | 1 | Broadcast scheduled | `{broadcast_id, scheduled_at, format}` |
| `broadcast.generating` | 1 | Broadcast generation started | `{broadcast_id}` |
| `broadcast.generated` | 1 | Broadcast generated | `{broadcast_id, segments, word_count, duration_ms}` |
| `broadcast.audio.generating` | 1 | Audio generation started | `{broadcast_id}` |
| `broadcast.audio.ready` | 1 | Audio file ready | `{broadcast_id, file_path, duration_seconds, file_size_bytes}` |
| `broadcast.breaking_news` | 1 | Breaking news interruption | `{broadcast_id, event_id, importance}` |
| `broadcast.error` | 1 | Broadcast generation failed | `{broadcast_id, error}` |

#### System Events

| Event Type | Version | Description | Payload |
|------------|---------|-------------|---------|
| `system.heartbeat` | 1 | Service heartbeat | `{service_name, status, uptime_seconds}` |
| `system.service.started` | 1 | Service started | `{service_name, version}` |
| `system.service.stopped` | 1 | Service stopped | `{service_name, reason}` |
| `system.service.crash` | 1 | Service crashed | `{service_name, error, stack_trace}` |
| `system.service.recovered` | 1 | Service recovered | `{service_name, recovery_time_ms}` |
| `system.maintenance.start` | 1 | Maintenance started | `{task_name}` |
| `system.maintenance.complete` | 1 | Maintenance complete | `{task_name, duration_ms}` |
| `system.snapshot.created` | 1 | System snapshot created | `{snapshot_path, size_bytes, components}` |
| `system.error` | 1 | Generic system error | `{component, error, severity}` |

### API Gateway (External)

The API Gateway exposes REST endpoints for external clients (dashboard, CLI, integrations).

```rust
pub struct ApiGateway {
    pub rest_port: u16,           // Default: 8080
    pub websocket_port: u16,     // Default: 8081
    pub cors_allowed_origins: Vec<String>,
    pub auth_enabled: bool,       // Default: false (local-first)
    pub rate_limit: RateLimitConfig,
}
```

#### REST Endpoints

**Dashboard:**
```
GET    /api/v1/events                    — List events (paginated, filterable)
GET    /api/v1/events/:id                — Get event details
GET    /api/v1/narratives                — List narratives
GET    /api/v1/narratives/:id            — Get narrative detail with events
GET    /api/v1/entities                  — List entities (searchable)
GET    /api/v1/entities/:id              — Get entity detail with claims
GET    /api/v1/contradictions            — List contradictions
GET    /api/v1/sources                   — List configured sources
GET    /api/v1/sources/:id               — Get source status
GET    /api/v1/broadcasts               — List recent broadcasts
GET    /api/v1/broadcasts/latest         — Get latest broadcast
GET    /api/v1/search                    — Full-text + vector search
GET    /api/v1/stats                     — System statistics
GET    /api/v1/health                    — System health summary
```

**Management:**
```
POST   /api/v1/sources                  — Add new source
PUT    /api/v1/sources/:id              — Update source config
DELETE /api/v1/sources/:id              — Remove source
POST   /api/v1/broadcasts/generate      — Trigger immediate broadcast
POST   /api/v1/events/:id/resolve       — Mark event as resolved
POST   /api/v1/contradictions/:id/resolve — Resolve contradiction
POST   /api/v1/entities/merge           — Merge two entities
GET    /api/v1/export                   — Export data (JSON/CSV/RDF)
```

**Configuration:**
```
GET    /api/v1/config                   — Get system configuration
PUT    /api/v1/config                   — Update configuration
GET    /api/v1/config/sources           — List source configurations
GET    /api/v1/config/models            — Model registry status
PUT    /api/v1/config/models/:id        — Change model assignment
POST   /api/v1/config/models/download   — Download new model
```

#### WebSocket API

WebSocket connections receive real-time events:

```json
// Client connects to ws://localhost:8081/ws?token=...
// Server pushes events as they happen:

{
  "type": "event",
  "data": {
    "event_type": "correlation.contradiction.detected",
    "payload": { "contradiction_id": "...", "severity": 0.8 }
  }
}

{
  "type": "broadcast",
  "data": {
    "broadcast_id": "...",
    "title": "Morning Briefing",
    "status": "generated"
  }
}

{
  "type": "system",
  "data": {
    "event_type": "system.heartbeat",
    "payload": { "services": { "ingestion": "healthy", "extraction": "healthy" } }
  }
}
```

**Client-to-server messages:**
```json
{
  "type": "subscribe",
  "channels": ["events", "broadcasts", "system"]
}

{
  "type": "unsubscribe",
  "channels": ["system"]
}

{
  "type": "ping"
}
```

### Service API Contracts

Services expose internal gRPC APIs for synchronous request-response patterns:

```protobuf
// Knowledge Graph Service
service KnowledgeGraph {
    rpc ExecuteQuery(QueryRequest) returns (QueryResponse);
    rpc ExecuteMutation(MutationRequest) returns (MutationResponse);
    rpc GetNode(NodeRequest) returns (NodeResponse);
    rpc GetNeighbors(NeighborsRequest) returns (NeighborsResponse);
    rpc SearchEntities(EntitySearchRequest) returns (EntitySearchResponse);
}

// Model Runtime Service
service ModelRuntime {
    rpc Infer(InferenceRequest) returns (InferenceResponse);
    rpc GetModelStatus(ModelStatusRequest) returns (ModelStatusResponse);
    rpc ListModels(ListModelsRequest) returns (ListModelsResponse);
}

// Document Store Service
service DocumentStore {
    rpc StoreDocument(StoreRequest) returns (StoreResponse);
    rpc GetDocument(GetDocumentRequest) returns (RawDocument);
    rpc DeleteDocument(DeleteRequest) returns (DeleteResponse);
    rpc SearchDocuments(DocumentSearchRequest) returns (DocumentSearchResponse);
}
```

### Query Parameters

Standard pagination and filtering:

```rust
pub struct PaginationParams {
    pub page: u32,        // Default: 1
    pub per_page: u32,    // Default: 20, Max: 100
    pub sort_by: String,  // Field to sort by
    pub sort_order: SortOrder, // ASC, DESC
}

pub struct FilterParams {
    pub status: Option<String>,
    pub event_type: Option<String>,
    pub importance_min: Option<f32>,
    pub importance_max: Option<f32>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub search: Option<String>,
    pub entity_id: Option<String>,
}
```

### Error Response Format

```json
{
  "error": {
    "code": "EVENT_NOT_FOUND",
    "message": "Event with id 01J2Y3Z4A5B6C7D8E9F0G1H2I3 not found",
    "details": {
      "event_id": "01J2Y3Z4A5B6C7D8E9F0G1H2I3"
    }
  }
}
```

**Error codes:**
- `INTERNAL_ERROR` — Unexpected server error
- `INVALID_REQUEST` — Malformed request
- `NOT_FOUND` — Resource not found
- `VALIDATION_ERROR` — Request validation failed
- `RATE_LIMITED` — Too many requests
- `SERVICE_UNAVAILABLE` — Dependent service down
- `CONFIGURATION_ERROR` — System configuration invalid

## Interfaces

- `plugin-api.md` — plugin extensions to internal API
- `docs/architecture/service-boundaries.md` — service boundaries that this API connects
- `docs/ui/dashboard-spec.md` — UI that consumes this API
- `docs/deployment/observability.md` — monitoring of API performance

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| API version mismatch | Client errors | Versioned endpoints; graceful degradation for old clients |
| WebSocket connection drop | Real-time updates lost | Client reconnection with state reconciliation |
| REST endpoint timeout | Slow dashboard | Configurable timeouts; async endpoints for heavy queries |
| Schema evolution breaking | Deserialization errors | Backward-compatible schema changes; version negotiation |
| Rate limiting too aggressive | Dashboard unusable | Configurable limits; per-endpoint limits for heavy queries |

## Future Extensions

- GraphQL endpoint for flexible dashboard queries
- OpenAPI/Swagger documentation generation
- API key authentication for external integrations
- Rate limiting per API key
- Request/response compression
- HTTP/2 streaming for real-time data
- Webhook subscriptions for external systems
