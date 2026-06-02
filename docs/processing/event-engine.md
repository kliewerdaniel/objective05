# Event Engine

## Purpose

Define the event engine — the subsystem responsible for forming, merging, evolving, and tracking real-world events from extracted claims and entities.

## Scope

This document covers event creation, event merging, event evolution over time, confidence updates, event completion detection, and event lifecycle management.

## Responsibilities

- Group related claims into coherent events
- Score event confidence based on supporting evidence
- Merge duplicate or overlapping events
- Track event evolution over time (new claims update events)
- Detect event completion (when no new claims are expected)
- Emit event-related events for downstream processing

## Assumptions

- Events are derived from claims (claims → events, not documents → events directly)
- Multiple claims from multiple sources can support a single event
- Events can span arbitrary time ranges (minutes to years)
- An event may evolve in significance, scope, and certainty over time
- Events are related but distinct from narratives (events are atomic; narratives are clusters of events)

## Design

### Event Formation Pipeline

```
New Claims (from Extraction)
    │
    ▼
┌──────────────────┐
│  Claim Intake     │  Receive new claims, validate, index
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Event Matching   │  Match claims to existing events or create new
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Confidence       │  Update event confidence based on new evidence
│  Update           │
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Event Merge      │  Merge overlapping events
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Event Scoring    │  Compute importance, source diversity, timeliness
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Graph Write      │  Update event nodes in knowledge graph
└──────┬───────────┘
       │
       ▼
    ┌──────┐
    │ Emit  │  correlation.event.created / updated
    └──────┘
```

### Event Matching

When a new claim arrives, the Event Engine determines if it belongs to an existing event or represents a new event.

**Matching criteria:**

| Criterion | Weight | Description |
|-----------|--------|-------------|
| Shared entities | 0.4 | Claims involve same entities |
| Temporal proximity | 0.2 | Claims reference same or nearby time |
| Location match | 0.2 | Claims reference same location |
| Topic/type match | 0.15 | Claims have same event topic/type |
| Source diversity | 0.05 | Bonus for matching different sources |

**Matching algorithm:**

```rust
pub fn match_claim_to_events(claim: &Claim, events: &[Event]) -> Vec<(EventId, f32)> {
    events
        .iter()
        .map(|event| {
            let score = compute_match_score(claim, event);
            (event.id, score)
        })
        .filter(|(_, score)| *score > MATCH_THRESHOLD)  // Default: 0.5
        .sorted_by(|(_, a), (_, b)| b.partial_cmp(a))
        .take(TOP_N_MATCHES)  // Default: 3
        .collect()
}
```

**Decision logic:**
- Best match > 0.7: Add claim to event
- Best match 0.5-0.7: Add to event with lower confidence; flag for review if low
- No match > 0.5: Create new event
- Claim matches multiple events > 0.5: Add to best match; flag events as potentially related

### Event Creation

When a claim doesn't match any existing event, a new event is created:

```rust
pub fn create_event_from_claims(claims: &[Claim]) -> Event {
    let event = Event {
        id: Uuid::new_v7(),
        title: generate_event_title(claims),      // LLM-generated concise title
        description: generate_description(claims), // LLM-generated summary
        event_type: infer_event_type(claims),
        status: EventStatus::Active,
        importance: initial_importance(claims),
        confidence: mean_confidence(claims),
        start_time: earliest_timestamp(claims),
        end_time: latest_timestamp(claims),
        location: extract_common_location(claims),
        participating_entities: extract_entities(claims),
        claim_count: claims.len(),
        source_diversity: compute_source_diversity(claims),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        valid_from: Utc::now(),
        valid_to: None,
        provenance: Provenance::from_claims(claims),
    };
    event
}
```

**Title generation prompt (conceptual):**

```
Generate a concise, informative title for an event based on these claims.
Title should be 5-10 words, include key entities and action.
Do not use passive voice.

Claims:
{claims_text}

Title:
```

### Confidence Update

Event confidence is updated whenever new claims are added:

```rust
pub fn update_event_confidence(event: &mut Event, new_claims: &[Claim]) {
    // Base confidence: mean of all claim confidences
    let all_claims = get_all_claims(event);
    let base_confidence = all_claims.iter().map(|c| c.confidence).sum::<f32>()
        / all_claims.len() as f32;

    // Boost from source diversity
    let unique_sources = count_unique_sources(all_claims);
    let diversity_factor = 1.0 + (0.1 * (unique_sources - 1).min(3) as f32);

    // Temporal decay for old events without new claims
    let hours_since_last_claim = Utc::now() - event.last_claim_at;
    let recency_factor = if hours_since_last_claim > 48 { 0.95_f32.powf(hours_since_last_claim as f32 / 24.0) }
                          else { 1.0 };

    event.confidence = (base_confidence * diversity_factor * recency_factor).min(1.0);
    event.source_diversity = diversity_factor / 1.3; // Normalize to [0, 1]
}
```

### Event Merging

When two events are determined to represent the same real-world occurrence, they are merged.

**Merge triggers:**
1. Same entity + same location + proximate time → high merge probability
2. User manually merges events via UI
3. Periodic merge scan (daily) finds overlapping events

**Merge algorithm:**

```rust
pub fn merge_events(primary: &mut Event, secondary: &Event) {
    // Primary retains its ID; secondary becomes a reference

    // Merge claims
    let secondary_claims = get_claims(secondary.id);
    reassign_claims_to_event(secondary_claims, primary.id);

    // Merge metadata
    primary.claim_count += secondary.claim_count;
    primary.source_diversity = max(primary.source_diversity, secondary.source_diversity);
    primary.start_time = min(primary.start_time, secondary.start_time);
    primary.end_time = max(primary.end_time, secondary.end_time);
    primary.importance = max(primary.importance, secondary.importance);
    primary.confidence = (primary.confidence + secondary.confidence) / 2.0;

    // Record merge in provenance
    primary.provenance.history.push(ProvenanceEntry {
        timestamp: Utc::now(),
        action: "event_merge",
        details: format!("Merged event {} into {}", secondary.id, primary.id),
    });

    // Mark secondary as superseded (logical deletion)
    secondary.valid_to = Some(Utc::now());
    secondary.status = EventStatus::Merged;
}
```

### Event Evolution

Events evolve through their lifecycle as new information arrives:

```
Creation → Active → Evolving → Stable → Resolved → Archived
                 ↘        ↗
               Updated with new claims
```

| Phase | Description | Conditions |
|-------|-------------|------------|
| Formation | Initial event, few claims | < 3 claims OR < 1 hour old |
| Active | Event is being actively updated | New claims in last 24 hours |
| Evolving | Event scope/significance changing | Significant new claims changing title/description |
| Stable | No new claims expected soon | > 48 hours since last claim |
| Resolved | Event concluded | Clear end marker (e.g., "CEO resigned" + "new CEO appointed") |
| Archived | Event moved to cold storage | > 30 days resolved |

**Evolution event types emitted:**
- `correlation.event.formed` — new event created
- `correlation.event.updated` — event metadata changed (title, confidence, etc.)
- `correlation.event.merged` — two events merged
- `correlation.event.status_changed` — event lifecycle transition
- `correlation.event.completed` — event resolved

### Importance Scoring

Event importance determines broadcast priority:

```rust
pub fn compute_event_importance(event: &Event) -> f32 {
    let evidence_volume = (event.claim_count as f32 / 50.0).min(1.0);   // 0-1, saturates at 50 claims
    let source_diversity = event.source_diversity;                         // 0-1
    let recency = (1.0 - hours_since_update / 168.0).max(0.0);            // 0-1, decays over 7 days
    let entity_prominence = mean_entity_importance(event.entities);       // 0-1, based on entity mention count
    let confidence = event.confidence;                                     // 0-1

    // Weighted combination
    0.3 * evidence_volume +
    0.25 * source_diversity +
    0.2 * recency +
    0.15 * entity_prominence +
    0.1 * confidence
}
```

### Periodic Maintenance

The Event Engine runs a periodic maintenance cycle (default: every 15 minutes):

1. Scan for events with no updates in 48 hours → mark as Stable
2. Scan for events with no updates in 7 days → mark as Resolved (if appropriate)
3. Scan for resolvable events (e.g., events with clear conclusion signals)
4. Scan for merge candidates (pairs of events with overlapping entities and time ranges)
5. Prune event confidence decay (re-apply decay for events without new claims)
6. Re-score importance for Active and Evolving events

### Input/Output Contracts

**Input:** Extraction events (from `extraction.*` topics)

```json
{
  "type": "extraction.claim.extracted",
  "data": {
    "claim_id": "uuid",
    "claim_text": "Apple reported revenue of $94.8B",
    "subject_id": "entity-uuid",
    "predicate": "reported_revenue",
    "object_value": "$94.8 billion",
    "confidence": 0.92,
    "source_document_id": "doc-uuid"
  }
}
```

**Output:** Event events (published to `correlation.event.*` topics)

```json
{
  "type": "correlation.event.updated",
  "data": {
    "event_id": "event-uuid",
    "title": "Apple Q2 2026 Earnings Report",
    "event_type": "business",
    "status": "evolving",
    "importance": 0.78,
    "confidence": 0.85,
    "claim_count": 12,
    "source_count": 4,
    "updated_fields": ["confidence", "importance", "claim_count"]
  }
}
```

## Interfaces

- `extraction-engine.md` — upstream claim producer
- `narrative-engine.md` — downstream event consumer
- `contradiction-engine.md` — independent consumer of claims
- `docs/data/knowledge-graph.md` — Event node and edge schemas

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Event match too aggressive | False event merges | Conservative match threshold; manual unmerge in UI |
| Event match too conservative | Event fragmentation | Periodic merge scan catches missed matches |
| Infinite event creation (spam) | Event explosion | Deduplicate similar events; minimum claim threshold (2) |
| Title generation poor quality | Confusing event titles | Fallback to template-based titles if LLM fails |
| Event merge conflicts | Lost information | Both event histories preserved via versioning |
| Cascade on reprocessing | Event thrashing | Debounce window before event updates take effect |

## Future Extensions

- Predictive event tracking (forecast event trajectory)
- Event branching (event splits into sub-events)
- Cross-event correlation (events that causally relate but don't merge)
- User-defined event types (taxonomy extensions)
- Event timeline visualization data
- External event calendar integration
