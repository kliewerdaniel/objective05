# Provenance Model

## Purpose

Define how Objective tracks the origin, confidence, evolution, and lineage of all information in the knowledge graph. This ensures every piece of information can be traced back to its source, evaluated for reliability, and understood in temporal context.

## Scope

This document covers source attribution, confidence tracking, temporal versioning, information lineage, and audit trails. It applies to all node and edge types in the knowledge graph.

## Responsibilities

- Define the provenance metadata model
- Specify confidence scoring and decay
- Define versioning and history tracking
- Document temporal query support
- Specify lineage tracking between derived information

## Assumptions

- Every node and edge has provenance metadata (see `knowledge-graph.md` schemas)
- Provenance is written at creation time and never mutated (appended only)
- Confidence is computed from source reliability, extraction quality, and supporting evidence count
- Temporal fields enable point-in-time queries and history reconstruction

## Design

### Provenance Data Model

Every node and edge in the knowledge graph carries the following provenance metadata:

```json
{
  "created_at": "2026-06-02T12:00:00Z",
  "updated_at": "2026-06-02T12:00:00Z",
  "valid_from": "2026-06-02T11:30:00Z",
  "valid_to": null,
  "provenance": {
    "sources": [
      {
        "source_id": "uuid-of-source",
        "document_id": "uuid-of-document",
        "extracted_at": "2026-06-02T12:00:00Z",
        "extraction_method": "ner_model_v2",
        "confidence": 0.85
      }
    ],
    "history": [
      {
        "timestamp": "2026-06-01T10:00:00Z",
        "previous_value": "Company X",
        "new_value": "Company X Corp",
        "reason": "source_update",
        "by": "entity_resolution"
      },
      {
        "timestamp": "2026-06-01T15:00:00Z",
        "previous_value": null,
        "new_value": 0.75,
        "reason": "evidence_accumulation",
        "by": "correlation_service"
      }
    ]
  }
}
```

### Field Semantics

| Field | Description |
|-------|-------------|
| `created_at` | When this node/edge was first created |
| `updated_at` | When this node/edge was last modified |
| `valid_from` | The time from which this information is known to be valid |
| `valid_to` | NULL if currently valid; timestamp when superseded or invalidated |
| `provenance.sources` | List of source documents that support this information |
| `provenance.history` | Ordered list of changes to this node/edge |

### Source Attribution

Every claim, entity, and relationship tracks its originating sources:

```
Claim "Revenue was $10B"
  ├── Source: SEC Filing (confidence: 0.95)
  │   └── Document: 10-K filing 2026-06-01
  └── Source: News Article (confidence: 0.7)
      └── Document: Bloomberg article 2026-06-02
```

**Attribution rules:**
1. Every extraction records at least one source document
2. When merging information from multiple sources, all sources are preserved
3. Sources are deduplicated by document_id
4. Source list is ordered by extraction time (most recent first)
5. Source count contributes to confidence computation

### Confidence Tracking

Confidence is a float in [0.0, 1.0] representing the system's certainty about a piece of information.

**Confidence computation for extractions:**

```
confidence = source_reliability * extraction_quality * evidence_factor
```

Where:
- `source_reliability`: Pre-configured reliability of the source (0-1)
  - Official sources (SEC, government): 0.95
  - Major news outlets (AP, Reuters, BBC): 0.85
  - Blogs: 0.5
  - Social media: 0.3
  - Unverified sources: 0.1
- `extraction_quality`: Confidence from the extraction model (0-1)
  - Entity extraction: model logprob normalized to [0,1]
  - Claim extraction: model confidence score
  - Relationship extraction: model confidence
- `evidence_factor`: Boost based on corroboration
  - 1 source: 1.0
  - 2 independent sources: 1.2
  - 3+ independent sources: 1.3
  - Capped at 1.0 final confidence

**Confidence for derived information:**

```
event_confidence = mean(claim_confidence) * source_diversity_factor
narrative_confidence = mean(event_confidence) * coherence_factor
contradiction_confidence = min(claim_a.confidence, claim_b.confidence) * contradiction_strength
```

**Confidence decay:**

```
decayed_confidence = confidence * exp(-lambda * days_since_last_verification)

lambda = 0.01  # Configurable decay rate
          # After 30 days: 0.74 * original
          # After 90 days: 0.41 * original
          # After 365 days: 0.03 * original
```

Confidence decay is applied:
1. When querying for current-state information
2. When generating broadcast content (stale information is deprioritized)
3. During maintenance cycles (low-confidence information may be archived)

Confidence is NOT decayed for historical queries (querying what was known at a point in time).

### Versioning

Every update to a node or edge creates a new version:

```
Version 1 (2026-06-01T10:00:00Z):
  Name: "Company X"
  Confidence: 0.7
  valid_from: 2026-06-01T10:00:00Z
  valid_to: 2026-06-02T08:00:00Z  ← Superseded

Version 2 (2026-06-02T08:00:00Z):
  Name: "Company X Corp (formerly Company X)"
  Confidence: 0.85
  valid_from: 2026-06-02T08:00:00Z
  valid_to: null  ← Current version
```

**Versioning rules:**
1. When a node/edge is updated, the previous version's `valid_to` is set to current timestamp
2. The new version's `valid_from` is set to current timestamp
3. Both versions remain in the database — no physical deletion
4. The version chain is reconstructable via `created_at` ordering
5. Maximum versions per node: 100 (configurable); oldest versions archived

**Update triggers (what causes a new version):**

| Trigger | Example |
|---------|---------|
| New information from higher-confidence source | SEC filing corrects a news article |
| Entity resolution merge | Two entity nodes merged into one |
| Confidence update | Evidence accumulation changes score |
| Manual correction | User edits entity name or claim |
| Contradiction resolution | Contradiction resolved in favor of one claim |
| Temporal change | Event start time updated with new information |

### Temporal History

The temporal model enables several query patterns:

**Current state query (default):**
```cypher
MATCH (e:Entity {id: $id})
WHERE e.valid_to IS NULL
RETURN e
```

**Point-in-time query:**
```cypher
MATCH (e:Entity {id: $id})
WHERE e.valid_from <= $point_in_time
  AND (e.valid_to IS NULL OR e.valid_to > $point_in_time)
RETURN e
```

**Time-range query (what changed between dates):**
```cypher
MATCH (e:Entity {id: $id})
WHERE e.updated_at >= $start_date
  AND e.updated_at <= $end_date
RETURN e
ORDER BY e.updated_at
```

**Full history query:**
```cypher
MATCH (e:Entity {canonical_name: $name})
RETURN e.canonical_name, e.confidence, e.valid_from, e.valid_to
ORDER BY e.valid_from
```

### Lineage Tracking

Lineage tracks how derived information (events, narratives, contradictions) relates to source information (claims, entities, documents).

```
Broadcast Script
  └── Based on Narrative "Supply Chain Crisis"
        └── Based on Events [Event_A, Event_B, Event_C]
              ├── Event_A based on Claims [C1, C2, C3]
              │     ├── C1 extracted from Document D1 (RSS - Reuters)
              │     ├── C2 extracted from Document D2 (SEC Filing)
              │     └── C3 extracted from Document D3 (Blog)
              ├── Event_B based on Claims [C4, C5]
              └── Event_C based on Claims [C6]
```

**Lineage metadata structure:**
```json
{
  "lineage": {
    "parent_ids": ["event-A", "event-B", "event-C"],
    "derivation_method": "narrative_clustering_v2",
    "parameters": {
      "similarity_threshold": 0.7,
      "min_events": 3
    },
    "input_version": "schema_v3",
    "confidence": 0.82
  }
}
```

**Lineage rules:**
1. Every derived node stores references to its parent nodes
2. Derivation method and parameters are recorded
3. Confidence of derived information references input confidences
4. If a parent node is invalidated (confidence drops below threshold), derived nodes are flagged for re-evaluation
5. Lineage depth is unlimited but practically bounded by pipeline stages (max 5-6)

### Audit Trail

All provenance changes are also written to an append-only audit log:

```json
{
  "timestamp": "2026-06-02T12:00:00Z",
  "action": "ENTITY_MERGE",
  "actor": "entity_resolution",
  "details": {
    "primary_entity_id": "entity-123",
    "merged_entity_id": "entity-456",
    "reason": "High similarity score (0.92)",
    "properties_merged": ["canonical_name", "aliases", "mention_count"]
  },
  "previous_state": { /* before merge */ },
  "new_state": { /* after merge */ }
}
```

The audit log is:
- Append-only (no deletions or modifications)
- Stored as JSON lines file rotated daily
- Retained for 90 days (configurable)
- Queryable through the API gateway

### Provenance API

Services interact with provenance through these operations:

```rust
pub trait ProvenanceManager {
    /// Record the creation of a new node/edge with provenance
    fn record_creation(&self, node: &Node, sources: &[SourceRef]) -> Result<()>;

    /// Update a node/edge, creating a new version
    fn record_update(&self, node: &Node, reason: &str) -> Result<()>;

    /// Invalidate a node/edge (set valid_to)
    fn record_invalidation(&self, node_id: &str, reason: &str) -> Result<()>;

    /// Merge two nodes, preserving both provenance chains
    fn record_merge(&self, primary: &Node, secondary: &Node, reason: &str) -> Result<()>;

    /// Get the full history of a node
    fn get_history(&self, node_id: &str) -> Result<Vec<NodeVersion>>;

    /// Get state at a specific point in time
    fn get_at_time(&self, node_id: &str, timestamp: &DateTime) -> Result<Option<Node>>;

    /// Get lineage (ancestors and descendants)
    fn get_lineage(&self, node_id: &str, direction: LineageDirection) -> Result<LineageTree>;
}
```

## Interfaces

- `knowledge-graph.md` — graph schemas that include provenance fields
- `docs/architecture/architecture-decisions.md` — ADR-007 (timestamp-priority merge) and ADR-011 (temporal graph)
- `docs/processing/extraction-engine.md` — how extraction records provenance
- `docs/processing/event-engine.md` — how event formation maintains lineage
- `docs/processing/contradiction-engine.md` — how contradictions reference claims

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Provenance data not written | Information untraceable | Write provenance before graph mutation; rollback on failure |
| Version chain exceeds limit | Performance degradation | Archive old versions; configurable version limit |
| Confidence decay misconfigured | Stale or suppressed information | Validate decay parameters at startup; monitoring alerts |
| Lineage cycle | Infinite recursion | Enforce DAG structure; detect cycles on write |
| Audit log full | Write failures | Log rotation with compression; alert on approaching limits |
| Timezone confusion | Temporal query errors | All timestamps in UTC; validate on input |

## Future Extensions

- Cryptographic proof of provenance (Merkle tree of version chain)
- External provenance verification (sign information with user key)
- Confidence calibration via human feedback
- Automated confidence recalibration based on prediction accuracy
- Provenance export for regulatory compliance (GDPR, SOX)
- Cross-instance provenance sharing with trust verification
