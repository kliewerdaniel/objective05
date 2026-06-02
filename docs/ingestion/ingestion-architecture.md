# Ingestion Architecture

## Purpose

Define the architecture of Objective's ingestion layer — the subsystem responsible for discovering, fetching, and normalizing information from external sources.

## Scope

This document covers source adapter architecture, polling mechanisms, scheduling, reliability guarantees, error handling, rate limiting, and document normalization. It does not cover specific source types (see `source-types.md`).

## Responsibilities

- Define the ingestion pipeline from source to raw document
- Specify the source adapter contract
- Document polling strategies (push vs pull)
- Define reliability and retry semantics
- Specify backpressure and rate limiting
- Document error classification and handling

## Assumptions

- Sources are primarily pull-based (HTTP, RSS, API polling)
- Push-based sources (webhooks) may be supported via the API gateway
- Network connectivity is not guaranteed
- Source availability is not guaranteed
- Document size is bounded at 10MB (larger documents are rejected)
- Source authentication credentials are stored with OS-level encryption

## Design

### Ingestion Pipeline

```
┌──────────┐    ┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ Scheduler │───▶│ Source       │───▶│ Fetcher      │───▶│ Normalizer   │
│ (trigger) │    │ Adapter      │    │              │    │              │
└──────────┘    │ (strategy)   │    │ (HTTP fetch, │    │ (parse,      │
                 │              │    │  API call,   │    │  validate,   │
                 │              │    │  read file)  │    │  transform)  │
                 └──────────────┘    └──────────────┘    └──────┬───────┘
                                                               │
                                                               ▼
                                                       ┌──────────────┐
                                                       │ Deduplicator │
                                                       │ (content hash│
                                                       │  + URL)      │
                                                       └──────┬───────┘
                                                              │
                              ┌───────────────────────────────┼───────────────┐
                              │                               │               │
                              ▼                               ▼               ▼
                      ┌──────────────┐               ┌──────────────┐  ┌──────────┐
                      │ Document     │               │ Event:       │  │ Metrics  │
                      │ Store        │               │ document.    │  │ & Logs   │
                      │ (compressed) │               │ received     │  │          │
                      └──────────────┘               └──────────────┘  └──────────┘
```

### Source Adapter Contract

Every source adapter implements the following interface:

```rust
#[async_trait]
pub trait SourceAdapter: Send + Sync {
    /// Unique identifier for this source instance
    fn name(&self) -> &str;

    /// Source type identifier (rss, reddit, youtube, etc.)
    fn source_type(&self) -> &str;

    /// Validate source configuration at startup
    fn validate(&self) -> Result<(), ValidationError>;

    /// Fetch new documents from the source
    /// Returns any documents received since the last poll
    async fn poll(&self, cursor: Option<Cursor>) -> Result<PollResult, PollError>;

    /// Fetch a single document by its external ID (for manual refresh)
    async fn fetch_one(&self, external_id: &str) -> Result<RawDocument, FetchError>;

    /// Check source health (connectivity, auth validity)
    async fn health(&self) -> Result<HealthStatus, HealthError>;

    /// Handle source-specific rate limiting
    fn rate_limit_config(&self) -> RateLimitConfig;
}
```

### Polling Strategy

**Periodic Polling (default):**
- Scheduler emits `ingestion.poll.<source_type>` events on configured intervals
- Source adapter fetches new documents since last cursor position
- Default interval: 60 minutes for RSS, 15 minutes for news APIs, 24 hours for SEC filings
- Configurable per source

**Cursor management:**
- Ingestion Service maintains a cursor per source (opaque string)
- For RSS: cursor is the timestamp of the last fetched item
- For APIs: cursor is the last page token or timestamp
- For filesystem: cursor is file modification timestamp
- Cursor is persisted to `state/ingestion.cursors` and snapshotted

**Incremental fetch:**
- Sources should return only new/changed documents since the cursor
- If a source does not support incremental fetch, the adapter deduplicates by content hash
- Duplicate rate is monitored; high duplicate rates trigger adapter review

```rust
pub struct PollResult {
    pub documents: Vec<RawDocument>,
    pub new_cursor: Option<Cursor>,
    pub has_more: bool,        // Pagination support
    pub poll_duration: Duration,
}
```

### Scheduling Configuration

```yaml
sources:
  - name: "reuters_world"
    type: "rss"
    url: "https://www.reuters.com/world/feed"
    poll_interval: 30          # minutes
    retry:
      max_attempts: 3
      backoff: "exponential"   # 1min, 2min, 4min
    timeout: 30                # seconds
    rate_limit:
      requests_per_minute: 10

  - name: "sec_filings"
    type: "sec_edgar"
    query: "ticker=AAPL"
    poll_interval: 1440        # daily
    retry:
      max_attempts: 5
      backoff: "linear"        # 1h, 2h, 3h, 4h, 5h
    timeout: 120               # 2 minutes
```

### Document Normalization

All source adapters produce documents in a normalized format:

```rust
pub struct RawDocument {
    pub id: String,                    // UUID v7
    pub source_id: String,             // Reference to Source node
    pub source_type: String,           // "rss", "reddit", etc.
    pub external_id: String,           // ID in source system
    pub url: Option<String>,           // Canonical URL
    pub title: Option<String>,
    pub body: String,                  // Extracted text content
    pub body_format: BodyFormat,       // PlainText, Markdown, HTML
    pub author: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub language: String,              // BCP-47 code
    pub content_hash: String,          // SHA-256 of body
    pub metadata: HashMap<String, Value>, // Source-specific metadata
    pub raw_bytes: Option<Vec<u8>>,    // Original bytes (for archival)
}
```

**Normalization steps:**
1. Fetch raw bytes from source
2. Parse source format (RSS XML, HTML, API JSON, PDF text)
3. Extract title, body, author, date from source-specific locations
4. Strip HTML tags, convert to plain text (or Markdown)
5. Detect language (fasttext or CLD3)
6. Compute content_hash (SHA-256 of normalized body)
7. Validate minimum content length (reject <100 chars)
8. Validate maximum content length (truncate at 100KB)
9. Emit normalized RawDocument

### Deduplication

Documents are deduplicated by:
1. **Exact URL match**: Same URL → skip (unless force_refresh)
2. **Content hash match**: Same body → skip (different URL, same content)
3. **Similarity match** (optional): Cosine similarity > 0.95 → skip (near-duplicate)

```rust
pub struct DedupResult {
    pub is_duplicate: bool,
    pub existing_document_id: Option<String>,
    pub match_type: Option<DedupMatchType>, // ExactUrl, ContentHash, Similarity
}
```

Deduplication configuration:
```yaml
deduplication:
  url_match: true
  content_hash: true
  similarity:
    enabled: false            # Expensive; enable only for high-volume sources
    threshold: 0.95
    model: "bge-small-en-v1.5"
```

### Error Handling

**Error classification:**

| Error | Severity | Action |
|-------|----------|--------|
| Network timeout | Transient | Retry with backoff (max 3) |
| HTTP 429 (rate limit) | Transient | Retry after Retry-After header |
| HTTP 5xx | Transient | Retry with backoff (max 3) |
| HTTP 4xx (auth) | Fatal | Disable source, alert user |
| HTTP 4xx (not found) | Skip | Log and continue |
| Parse error | Skippable | Log, skip document, continue |
| Validation error | Skippable | Log, skip document, continue |
| Content too large | Skippable | Truncate or skip based on config |

**Retry strategy:**
```rust
pub enum RetryStrategy {
    Exponential {
        initial_delay: Duration,   // Default: 1 minute
        max_delay: Duration,        // Default: 30 minutes
        multiplier: f64,            // Default: 2.0
        jitter: f64,                // Default: 0.1 (10% randomness)
    },
    Linear {
        delay: Duration,            // Fixed delay between retries
    },
    None,                           // No retry on failure
}
```

### Rate Limiting

Each source adapter declares its rate limit policy:

```yaml
rate_limits:
  per_source:
    rss_generic:
      requests_per_minute: 30
      burst: 5
    reddit_api:
      requests_per_minute: 60
      burst: 10
    sec_edgar:
      requests_per_second: 10
      burst: 20
```

The Ingestion Service enforces rate limits using a token bucket algorithm:

```rust
pub struct TokenBucket {
    capacity: u32,
    tokens: f64,
    refill_rate: f64,     // tokens per second
    last_refill: Instant,
}
```

### Backpressure

If downstream services cannot keep up with ingestion:
1. Ingestion Service continues polling (documents are buffered in queue)
2. Queue depth is monitored
3. If queue exceeds threshold (configurable, default 10,000), polling slows
4. Backpressure signal is sent via message bus
5. When queue drains below threshold, normal polling resumes

### Source Lifecycle

```
States: DISABLED → ACTIVE → ERROR → DISABLED
                ↕         ↕
              VALIDATING  BACKOFF
```

| State | Description |
|-------|-------------|
| DISABLED | Source configured but not polling |
| VALIDATING | Source configuration being validated |
| ACTIVE | Source polling normally |
| ERROR | Source encountered a fatal error |
| BACKOFF | Source in retry backoff (transient error) |

Transitions:
- DISABLED → VALIDATING: On system start or manual enable
- VALIDATING → ACTIVE: Validation successful
- VALIDATING → DISABLED: Validation failed
- ACTIVE → BACKOFF: Transient error
- BACKOFF → ACTIVE: Retry successful
- ACTIVE → ERROR: Fatal error (auth failure, removed source)
- ERROR → DISABLED: Manual intervention required
- BACKOFF → ERROR: Max retries exhausted

### Monitoring

Per-source metrics exposed:
```
objective_ingestion_polls_total{source="reuters_world"} 1024
objective_ingestion_polls_failed_total{source="reuters_world"} 12
objective_ingestion_documents_received_total{source="reuters_world"} 8432
objective_ingestion_documents_duplicate_total{source="reuters_world"} 231
objective_ingestion_documents_errors_total{source="reuters_world"} 5
objective_ingestion_poll_duration_seconds{source="reuters_world"} 2.3
objective_ingestion_queue_depth 342
objective_ingestion_backpressure_active 0
```

## Interfaces

- `source-types.md` — specific source adapter implementations
- `docs/architecture/service-boundaries.md` — Ingestion Service in context
- `docs/data/storage-architecture.md` — document archive storage
- `docs/processing/extraction-engine.md` — downstream consumer of documents

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| All sources timeout | No new documents | Reduce parallelism, increase timeout |
| One source goes rogue (infinite data) | Resource exhaustion | Document size limits, per-source quotas |
| Credential leak | Security breach | OS-level credential encryption, audit logging |
| Source returns malicious content | Pipeline injection | Content validation, sanitization |
| Clock skew | Cursor management errors | NTP dependency, tolerance windows |
| Network partition | Complete ingestion halt | Graceful degradation, queue remaining documents |

## Future Extensions

- Push-based ingestion via webhook endpoint
- Proxy support for restricted networks
- Content negotiation (request specific formats)
- Adaptive polling (decrease interval for active sources, increase for inactive)
- Source health scoring (automatically disable unreliable sources)
- Federated source discovery (discover sources from other Objective instances)
- Scheduled bulk import for initial data loading
