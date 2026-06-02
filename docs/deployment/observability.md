# Observability

## Purpose

Define the observability infrastructure for Objective — metrics, logging, tracing, and health monitoring that enables understanding of system behavior, debugging issues, and measuring performance.

## Scope

This document covers metrics collection, log management, distributed tracing, health monitoring, alerting, and dashboards.

## Responsibilities

- Define metrics collection architecture
- Specify log formats and management
- Define tracing strategy
- Document health monitoring
- Specify alerting rules
- Define observability dashboards

## Assumptions

- Observability infrastructure runs locally (no cloud dependency)
- Metrics are stored in local time-series database (or in-memory for MVP)
- Logs are text files with structured format
- Tracing uses OpenTelemetry for vendor-neutral instrumentation
- Dashboard is served by Objective's API gateway

## Design

### Metrics Architecture

```
┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│  Services    │────▶│  Metrics     │────▶│  Exporters    │
│  (emit       │     │  Registry    │     │              │
│   metrics)   │     │  (in-memory) │     │  /metrics    │
└─────────────┘     └──────────────┘     │  endpoint    │
                                         │  (Prometheus) │
                                         └──────────────┘
```

**Metrics format:** OpenMetrics / Prometheus exposition format.

**Metrics endpoint:** `GET /api/v1/metrics` (Prometheus scrape target)

### Key Metrics

**System:**
```
objective_uptime_seconds{version="1.0.0"} 123456
objective_build_info{version="1.0.0", commit="abc1234", built="2026-06-01"} 1
objective_memory_used_bytes{type="rss"} 4294967296
objective_memory_used_bytes{type="heap"} 2147483648
objective_cpu_usage_percent 45.2
objective_disk_used_bytes{path="/var/lib/objective"} 8796093022
objective_disk_total_bytes{path="/var/lib/objective"} 107374182400
objective_disk_usage_percent{path="/var/lib/objective"} 8.2
objective_golang_gc_seconds_total 123.4
```

**Ingestion:**
```
objective_ingestion_polls_total{source="reuters_world"} 1024
objective_ingestion_polls_failed_total{source="reuters_world"} 12
objective_ingestion_poll_duration_seconds{source="reuters_world"} 2.3
objective_ingestion_documents_received_total{source="reuters_world"} 8432
objective_ingestion_documents_duplicate_total{source="reuters_world"} 231
objective_ingestion_documents_error_total{source="reuters_world"} 5
objective_ingestion_document_size_bytes{source="reuters_world"} 45230
objective_ingestion_queue_depth 342
objective_ingestion_backpressure_active 0
objective_ingestion_source_status{source="reuters_world", status="active"} 1
```

**Extraction:**
```
objective_extraction_documents_processed_total 8432
objective_extraction_documents_failed_total 23
objective_extraction_document_duration_seconds{doc_id="..."} 15.2
objective_extraction_entities_extracted_total 45231
objective_extraction_claims_extracted_total 28912
objective_extraction_relationships_extracted_total 12304
objective_extraction_entities_merged_total 1230
objective_extraction_claims_merged_total 4567
objective_extraction_queue_depth 15
objective_extraction_chunk_count_total 12345
objective_extraction_empty_result_total 234
```

**Correlation:**
```
objective_correlation_events_created_total 456
objective_correlation_events_updated_total 1234
objective_correlation_events_merged_total 45
objective_correlation_narratives_formed_total 89
objective_correlation_narratives_updated_total 567
objective_correlation_contradictions_detected_total 67
objective_correlation_contradictions_resolved_total 23
objective_correlation_maintenance_duration_seconds 45.6
objective_correlation_event_match_duration_seconds 0.023
```

**Broadcast:**
```
objective_broadcast_generated_total{format="brief"} 89
objective_broadcast_generated_total{format="full"} 45
objective_broadcast_generated_total{format="audio"} 67
objective_broadcast_generation_duration_seconds 120.5
objective_broadcast_audio_generation_duration_seconds 45.3
objective_broadcast_segments_per_broadcast 5.2
objective_broadcast_idle_broadcasts_total 12
objective_broadcast_breaking_news_total 7
```

**Model Runtime:**
```
objective_model_inference_total{model="mistral-7b", task="entity_extraction"} 12345
objective_model_inference_total{model="mistral-7b", task="claim_extraction"} 23456
objective_model_inference_total{model="mixtral-8x7b", task="report_generation"} 345
objective_model_inference_duration_seconds{model="mistral-7b"} 3.2
objective_model_inference_duration_seconds{model="mixtral-8x7b"} 45.6
objective_model_inference_tokens_total{model="mistral-7b"} 9876543
objective_model_inference_tokens_total{model="mixtral-8x7b"} 1234567
objective_model_inference_errors_total{model="mistral-7b"} 12
objective_model_inference_queue_depth{model="mixtral-8x7b"} 2
objective_model_memory_bytes{model="mistral-7b"} 4294967296
objective_model_memory_bytes{model="mixtral-8x7b"} 8589934592
objective_model_status{model="mistral-7b", status="ready"} 1
objective_model_parse_success_rate{model="mistral-7b", task="entity_extraction"} 0.97
```

**Health:**
```
objective_health_check_duration_seconds 0.005
objective_health_service_status{service="ingestion"} 1  # 1 = healthy
objective_health_service_status{service="extraction"} 1
objective_health_service_status{service="correlation"} 1
objective_health_service_status{service="broadcast"} 1
objective_health_service_status{service="model_runtime"} 1
objective_health_service_status{service="queue"} 1
objective_health_service_crashes_total{service="ingestion"} 0
objective_health_service_crashes_total{service="extraction"} 1
objective_health_service_restarts_total{service="extraction"} 1
```

### Logging

**Log format (JSON structured):**

```json
{
  "timestamp": "2026-06-02T12:00:00.123Z",
  "level": "INFO",
  "service": "ingestion",
  "source": "reuters_world",
  "message": "Poll completed successfully",
  "fields": {
    "documents_fetched": 12,
    "duplicates": 3,
    "errors": 0,
    "duration_ms": 2345
  },
  "error": null,
  "trace_id": "abc123def456",
  "span_id": "789ghi",
  "correlation_id": "01J2Y3Z4A5B6C7D8E9F0G1H2I3"
}
```

**Log levels:**

| Level | Usage |
|-------|-------|
| ERROR | Service failure, data loss, crashes |
| WARN | Degraded performance, retry exhaustion, configuration issues |
| INFO | Normal operations: polls, extractions, broadcasts |
| DEBUG | Detailed operational information (off by default) |
| TRACE | Per-call tracing (off by default, high volume) |

**Log configuration:**
```yaml
logging:
  level: "INFO"              # Default log level
  format: "json"             # json, text
  output: "file"             # file, stdout, stderr
  directory: "~/.objective/logs"
  rotation:
    max_size_mb: 100         # Rotate at 100MB
    max_files: 10            # Keep 10 rotated files
    max_age_days: 30         # Delete after 30 days
  fields:                    # Additional fields to include
    service: true
    source: true
    trace_id: true
```

**Log categories:**
- `objective.log` — all services (INFO+)
- `ingestion.log` — ingestion service only
- `extraction.log` — extraction service only
- `correlation.log` — correlation service only
- `broadcast.log` — broadcast service only
- `system.log` — system events, crashes, recoveries
- `audit.log` — append-only audit trail (can't be disabled)

### Tracing

Objective uses OpenTelemetry for distributed tracing:

```yaml
tracing:
  enabled: true
  exporter: "console"     # console, otlp, file
  sample_rate: 0.1        # Sample 10% of traces (0.0-1.0)
  service_name: "objective"
  attributes:
    environment: "production"
```

**Span naming conventions:**

| Span Name | Service |
|-----------|---------|
| `ingestion.poll` | Ingestion |
| `ingestion.document.fetch` | Ingestion |
| `extraction.document.process` | Extraction |
| `extraction.entity.extract` | Extraction |
| `extraction.claim.extract` | Extraction |
| `correlation.event.match` | Correlation |
| `correlation.narrative.cluster` | Correlation |
| `correlation.contradiction.check` | Correlation |
| `broadcast.generate` | Broadcast |
| `broadcast.audio.generate` | Broadcast |
| `model.inference` | Model Runtime |
| `graph.query` | Knowledge Graph |
| `graph.mutation` | Knowledge Graph |

**Trace propagation:**
- Across service boundaries via NATS message headers
- Correlation ID ties related spans across services
- Causation ID tracks parent-child span relationships

### Health Monitoring

**Health check endpoint:**
- `GET /api/v1/health` — returns aggregated health status
- `GET /api/v1/health/{service}` — returns individual service health
- Check interval: 10 seconds (internal), 30 seconds (external)

**Health states:**

| State | Description | HTTP Code |
|-------|-------------|-----------|
| healthy | All systems operational | 200 |
| degraded | Non-critical service degraded | 200 (with warning) |
| unhealthy | Critical service unavailable | 503 |

### Alerting Rules

```yaml
alerts:
  # System alerts
  - name: "HighMemoryUsage"
    condition: "objective_memory_used_bytes > 25GB"
    severity: "warning"
    action: "log + dashboard notification"
    
  - name: "CriticalMemoryUsage"
    condition: "objective_memory_used_bytes > 30GB"
    severity: "critical"
    action: "log + dashboard notification + restart models"
    
  - name: "DiskSpaceLow"
    condition: "objective_disk_usage_percent > 85"
    severity: "warning"
    action: "log + dashboard notification"
    
  - name: "DiskSpaceCritical"
    condition: "objective_disk_usage_percent > 95"
    severity: "critical"
    action: "log + dashboard notification + stop non-critical services"
    
  # Ingestion alerts
  - name: "SourceErrors"
    condition: "objective_ingestion_polls_failed_total > 5"
    severity: "warning"
    action: "disable source + notify user"
    
  - name: "NoNewDocuments"
    condition: "objective_ingestion_documents_received_total = 0 for 24h"
    severity: "warning"
    action: "notify user — check sources"
    
  # Extraction alerts
  - name: "ExtractionBacklog"
    condition: "objective_extraction_queue_depth > 100"
    severity: "warning"
    action: "increase extraction concurrency"
    
  - name: "ExtractionFailureRate"
    condition: "objective_extraction_documents_failed_total / objective_extraction_documents_processed_total > 0.1"
    severity: "warning"
    action: "review extraction model"
    
  # Model alerts
  - name: "ModelUnavailable"
    condition: "objective_model_status{status='ready'} == 0"
    severity: "critical"
    action: "attempt model reload + notify user"
    
  - name: "HighModelErrorRate"
    condition: "objective_model_inference_errors_total / objective_model_inference_total > 0.05"
    severity: "warning"
    action: "review model + prompt"
    
  # Broadcast alerts
  - name: "NoBroadcastsGenerated"
    condition: "objective_broadcast_generated_total = 0 for 24h"
    severity: "critical"
    action: "notify user — broadcast engine may be stuck"
```

### Dashboards

**Dashboard views (in Objective UI):**

| Dashboard | Description |
|-----------|-------------|
| System Overview | CPU, memory, disk, uptime, service health |
| Ingestion | Poll success rate, documents fetched, queue depth, source health |
| Extraction | Documents processed, entities/claims extracted, duration, queue |
| Correlation | Events/narratives/contradictions created, match latency |
| Broadcast | Broadcasts generated, duration, format distribution, idle ratio |
| Model Performance | Inference duration, tokens, queue depth, error rate, parse success rate |
| Storage | Disk usage by subsystem, document archive size, snapshot sizes |

## Interfaces

- `operations.md` — operational procedures using observability data
- `docs/api/internal-api.md` — metrics and health endpoints

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Metrics registry memory leak | Application OOM | Bounded metric cardinality; periodic reset of stale metrics |
| Log file fills disk | System degradation | Rotation and size limits; compression |
| Trace sampling too high | Performance impact | Default 10% sample rate; configurable |
| Health check false positive | Unnecessary alerts | Consecutive failures (3) before alert; configurable thresholds |
| Health check false negative | Missed failures | Redundant health checks from multiple perspectives |
| Excessive metrics cardinality | Memory/bloat | Label validation; limit unique label combinations |

## Future Extensions

- Structured logging with OpenTelemetry log export
- Grafana dashboard templates for advanced users
- Prometheus rule-based alerting with notification channels
- Long-term metrics storage with downsampling
- SLO monitoring for service reliability
- Distributed tracing visualization (Jaeger)
- Anomaly detection in metric patterns
- Custom metrics from plugin system
