# Plugin API

## Purpose

Define Objective's plugin architecture — how third-party developers can extend the system with custom source connectors, processors, broadcast formats, and other extensions without modifying the core.

## Scope

This document covers the plugin system architecture, extension points, source connectors, custom processors, plugin lifecycle, and the plugin gRPC contract.

## Responsibilities

- Define the plugin architecture and extension model
- Specify all extension points
- Document the plugin gRPC contract
- Define plugin lifecycle management
- Specify plugin packaging and discovery
- Document security and sandboxing

## Assumptions

- Plugins are separate processes (not shared libraries)
- Plugins communicate via gRPC (see ADR-010)
- Plugins may be written in any language with gRPC support
- The plugin API version is decoupled from the core version
- Plugin failures must not crash the core system

## Design

### Plugin Architecture

```
┌──────────────────────────────────────────┐
│         Objective Core System             │
│                                           │
│  ┌────────────────┐                       │
│  │  Plugin Host   │──── gRPC ────┐        │
│  │  (lifecycle,   │              │        │
│  │   discovery,   │              │        │
│  │   routing)     │              │        │
│  └────────────────┘              │        │
│                                  │        │
└──────────────────────────────────┼────────┘
                                   │
                          ┌────────┴────────┐
                          │  Plugin Process  │
                          │                  │
                          │  ├─ Source       │
                          │  ├─ Processor    │
                          │  └─ Broadcaster  │
                          │                  │
                          │  Any language    │
                          └─────────────────┘
```

**Plugin Host responsibilities:**
1. Discover plugins in configured directories
2. Start plugin processes
3. Manage gRPC connections
4. Route events to subscribed plugins
5. Handle plugin crashes (restart with backoff)
6. Validate plugin output
7. Expose plugin status via UI/API

### Extension Points

| Extension Point | Interface | Description | Example |
|----------------|-----------|-------------|---------|
| Source Adapter | `SourcePlugin` | Custom data source | Bloomberg Terminal API |
| Document Processor | `ProcessorPlugin` | Transform documents | Language translator |
| Entity Enricher | `ProcessorPlugin` | Enrich entities with external data | Wikipedia link resolver |
| Claim Validator | `ProcessorPlugin` | Validate claims against external DB | Fact-checking API |
| Broadcast Formatter | `BroadcastPlugin` | Custom broadcast output | PDF report generator |
| Notification Sink | `NotificationPlugin` | Send notifications | Slack/Email/Webhook |
| Embedding Provider | `EmbeddingPlugin` | Custom embedding model | Domain-specific embeddings |
| Content Filter | `FilterPlugin` | Filter content before processing | Spam filter, NSFW filter |

### Plugin gRPC Contract

```protobuf
syntax = "proto3";

package objective.plugin.v1;

// Core plugin identity and health
service Plugin {
    // Core identity — called at startup
    rpc GetInfo(Empty) returns (PluginInfo);

    // Health check — called periodically (default: 30s)
    rpc CheckHealth(Empty) returns (HealthStatus);
}

// Source adapter plugin
service SourcePlugin {
    rpc Validate(ValidateRequest) returns (ValidateResponse);
    rpc Poll(PollRequest) returns (stream DocumentEvent);
    rpc FetchOne(FetchOneRequest) returns (DocumentEvent);
    rpc GetStatus(Empty) returns (SourceStatus);
}

// Document processor plugin
service ProcessorPlugin {
    rpc Process(ProcessRequest) returns (ProcessResponse);
    rpc GetCapabilities(Empty) returns (ProcessorCapabilities);
}

// Broadcast formatter plugin
service BroadcastPlugin {
    rpc Format(FormatRequest) returns (FormatResponse);
    rpc GetCapabilities(Empty) returns (BroadcasterCapabilities);
}

// Notification plugin
service NotificationPlugin {
    rpc Notify(NotifyRequest) returns (NotifyResponse);
    rpc GetCapabilities(Empty) returns (NotificationCapabilities);
}

// Embedding provider plugin
service EmbeddingPlugin {
    rpc Embed(EmbedRequest) returns (EmbedResponse);
    rpc GetDimension(Empty) returns (DimensionResponse);
}

// --- Message Types ---

message PluginInfo {
    string name = 1;
    string version = 2;
    string description = 3;
    string author = 4;
    string plugin_type = 5;     // "source", "processor", "broadcast", "notification", "embedding"
    uint32 api_version = 6;     // Plugin API version
    repeated string capabilities = 7;
}

message HealthStatus {
    enum Status {
        UNKNOWN = 0;
        HEALTHY = 1;
        DEGRADED = 2;
        UNHEALTHY = 3;
    }
    Status status = 1;
    string message = 2;
    map<string, string> metrics = 3;
}

message ValidateRequest {
    string config_json = 1;     // Plugin-specific configuration as JSON
}

message ValidateResponse {
    bool valid = 1;
    repeated string errors = 2;
}

message PollRequest {
    string config_json = 1;
    string cursor = 2;          // Opaque cursor from last poll
}

message DocumentEvent {
    string external_id = 1;
    string url = 2;
    string title = 3;
    string body = 4;
    string body_format = 5;     // "plaintext", "markdown", "html"
    string author = 6;
    string published_at = 7;    // ISO8601
    string language = 8;        // BCP-47
    string cursor = 9;          // New cursor for incremental fetch
    string content_hash = 10;   // SHA-256
    string metadata_json = 11;  // Source-specific metadata as JSON
}

message FetchOneRequest {
    string config_json = 1;
    string external_id = 2;
}

message ProcessRequest {
    string document_id = 1;
    string title = 2;
    string body = 3;
    string body_format = 4;
    string metadata_json = 5;
    string config_json = 6;
}

message ProcessResponse {
    repeated EntityOutput entities = 1;
    repeated ClaimOutput claims = 2;
    map<string, string> metadata = 3;  // Updated metadata
}

message EntityOutput {
    string name = 1;
    string entity_type = 2;     // "Person", "Organization", "Location", "Concept", "Event_Topic"
    repeated string aliases = 3;
    string description = 4;
    double confidence = 5;
    string evidence_snippet = 6;
}

message ClaimOutput {
    string claim_text = 1;
    string subject_name = 2;
    string predicate = 3;
    string object_name = 4;
    string object_value = 5;
    string claim_type = 6;
    double confidence = 7;
    string evidence_snippet = 8;
}

message FormatRequest {
    string broadcast_id = 1;
    string content_json = 2;    // Broadcast content as structured JSON
    string format = 3;          // "brief", "full", "audio_script", "custom"
    string config_json = 4;
}

message FormatResponse {
    string output = 1;          // Formatted output
    string output_format = 2;   // "markdown", "html", "json", "pdf", etc.
    bytes output_binary = 3;    // For binary formats (PDF, image)
}

message NotifyRequest {
    string notification_type = 1; // "broadcast_ready", "breaking_news", "contradiction", "system_alert"
    string title = 2;
    string body = 3;
    string link = 4;
    string config_json = 5;
}

message NotifyResponse {
    bool delivered = 1;
    string provider_message = 2;
}

message EmbedRequest {
    repeated string texts = 1;
    string config_json = 2;
}

message EmbedResponse {
    repeated float embeddings = 1; // Flattened [text1_dim1, text1_dim2, ..., textN_dimM]
    uint32 dimension = 2;
}

message DimensionResponse {
    uint32 dimension = 1;
}

message ProcessorCapabilities {
    bool can_extract_entities = 1;
    bool can_extract_claims = 2;
    bool can_enrich_metadata = 3;
    repeated string supported_languages = 4;
}

message BroadcasterCapabilities {
    repeated string supported_formats = 1;
    bool supports_binary = 2;
}

message NotificationCapabilities {
    repeated string supported_types = 1;
    bool supports_markdown = 2;
    bool supports_attachments = 3;
}
```

### Plugin Discovery

Plugins are discovered from well-known directories:

```yaml
plugin:
  directories:
    - "~/.objective/plugins"
    - "/usr/local/lib/objective/plugins"
    - "/opt/objective/plugins"
  manifest: "plugin.json"     # Plugin manifest file
  auto_start: true
  restart_on_crash: true
  max_restart_backoff: 300    # 5 minutes max backoff
  health_check_interval: 30   # seconds
```

**Plugin manifest (`plugin.json`):**

```json
{
  "name": "bloomberg-source",
  "version": "1.0.0",
  "description": "Bloomberg Terminal data source",
  "author": "Community Developer",
  "plugin_type": "source",
  "api_version": 1,
  "executable": "./bloomberg-source-plugin",
  "args": ["--config", "config.yaml"],
  "env": {
    "BLOOMBERG_API_HOST": "localhost:8194"
  },
  "permissions": ["network"],
  "capabilities": ["poll", "fetch_one"],
  "config_schema": {
    "type": "object",
    "properties": {
      "api_key": { "type": "string" },
      "securities": { "type": "array", "items": { "type": "string" } }
    },
    "required": ["api_key"]
  }
}
```

### Plugin Lifecycle

```
Discovered → Validated → Started → Health Check → Ready
                                        │
                                        ▼
                                    Running ←→ Crashed
                                        │         │
                                        ▼         ▼
                                    Stopped    Restart (backoff)
```

| State | Description |
|-------|-------------|
| Discovered | Plugin manifest found in search directories |
| Validated | Manifest validated, config checked, executable verified |
| Started | Process spawned, gRPC connection established |
| Ready | Plugin health check returned HEALTHY |
| Running | Plugin actively processing |
| Crashed | Plugin process exited unexpectedly |
| Stopped | Plugin terminated gracefully |
| Error | Plugin in unrecoverable error state |

### Plugin Isolation

| Concern | Approach |
|---------|----------|
| Process isolation | Plugins run as separate OS processes |
| Resource limits | OS-level cgroups/rlimits (Linux), proc limit (macOS) |
| CPU limit | `--cpu-quota` (Docker), `cpulimit`, or OS scheduler |
| Memory limit | `--memory-max` via rlimit |
| Network access | Declared in manifest `permissions`; blocked if not declared |
| Filesystem access | Plugin data directory only (sandboxed) |
| Crash isolation | Plugin Host detects crash, restarts with backoff; core unaffected |
| Malicious output | Output validated against schema; rejected if invalid |

### Plugin Events

Plugins subscribe to core events and can emit their own events:

**Subscription:** Plugins declare event subscriptions in manifest:

```json
{
  "subscriptions": {
    "sources": ["extraction.entity.extracted"],
    "filters": {
      "entity_type": ["Person", "Organization"]
    }
  }
}
```

**Emitting events:** Plugins can emit events back to the core through a gRPC streaming endpoint:

```protobuf
service PluginEventBus {
    rpc Subscribe(Subscription) returns (stream Event);
    rpc Emit(Event) returns (EmitResponse);
}
```

### Source Connector Pattern

Custom source adapters follow this pattern:

```
1. Core sends PollRequest with config + cursor
2. Plugin fetches data from its source
3. Plugin streams DocumentEvent messages back
4. Core receives each DocumentEvent as a new document
5. Core sends cursor from last DocumentEvent on next poll
6. Plugin handles errors by returning error status in stream
```

**Best practices:**
- Reuse cursor for incremental fetching
- Handle rate limiting internally
- Respect `poll_interval` from core
- Validate configuration in `Validate()` handler
- Return HEALTHY only when source is reachable

### Processor Plugin Pattern

Custom processors run as a stage in the extraction pipeline:

```
1. Core sends ProcessRequest with document
2. Plugin processes (extracts entities, claims, enriches)
3. Plugin returns EntityOutput[] and ClaimOutput[]
4. Core merges plugin output with its own extraction
5. Plugin can also return updated metadata in metadata map
```

**Execution ordering:**
1. Core extraction (entity, claim, relationship)
2. Processor plugins (in declared order)
3. Core deduplication and merge
4. Graph write

### Broadcast Plugin Pattern

Custom broadcast formatters:

```
1. Core sends FormatRequest with broadcast content + format
2. Plugin transforms content to desired format
3. Plugin returns formatted output (text or binary)
4. Core stores output alongside standard broadcasts
```

### Security

```yaml
plugin_security:
  allow_network: false              # Default: no network access
  allow_filesystem_write: false     # Default: read-only
  allow_subprocesses: false         # Default: no subprocesses
  max_memory_mb: 512                # Memory limit per plugin
  max_cpu_percent: 50               # CPU limit per plugin
  timeout_seconds: 30               # gRPC call timeout
  signature_verification: false     # Optional plugin signing
  allowed_hosts: ["api.example.com"] # Network allowlist
```

**Security model:**
- Plugins run with least privilege
- Network access declared in manifest; enforced by firewall
- Plugin output validated ≥ core output validation
- Plugin crashes don't cascade to core
- Plugins have no access to knowledge graph directly (only through core API)

## Interfaces

- `internal-api.md` — core event types that plugins can subscribe to
- `docs/ingestion/source-types.md` — guidance for building source plugins
- `docs/processing/extraction-engine.md` — guidance for processor plugins
- `docs/broadcast/broadcast-engine.md` — guidance for broadcast plugins

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Plugin crashes | Plugin capability lost | Auto-restart with backoff; alert if crashes > 5 in 1hr |
| Plugin memory leak | System memory pressure | Memory limit enforced; plugin killed and restarted |
| Plugin hangs on gRPC call | Delayed processing | gRPC timeout (default 30s); kill unresponsive plugin |
| Malformed plugin output | Invalid data in graph | Schema validation on all plugin output |
| Malicious plugin | Data exfiltration or damage | Permission system; sandboxing; network restrictions |
| Plugin version incompatible | Start failure | API version negotiation; clear error on mismatch |

## Future Extensions

- Plugin marketplace and discovery service
- Signed plugin verification
- Plugin dependency management
- Plugin hot-reload (update without restart)
- Plugin resource monitoring dashboard
- Plugin development SDK and templates
- Community plugin repository
- Plugin test framework (mock core for plugin testing)
- Visual plugin editor
- Plugin performance benchmarking
