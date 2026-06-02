# Narrative Engine

## Purpose

Define the narrative engine — the subsystem responsible for forming, clustering, scoring, and tracking narratives (coherent storylines) from related events.

## Scope

This document covers narrative formation, event clustering, narrative scoring, narrative evolution over time, narrative lifecycle, and narrative relationship tracking.

## Responsibilities

- Cluster related events into coherent narratives
- Score narrative strength based on coverage, diversity, and recency
- Track narrative evolution over time (growth, decline, branching)
- Detect narrative forks and merges
- Rank narratives by significance
- Emit narrative-related events for broadcast and UI

## Assumptions

- Narratives are clusters of events (events → narratives, not directly from claims)
- Narratives have temporal extent (they begin, evolve, and may end)
- Narratives can have sub-narratives and parent narratives
- A single event can belong to multiple narratives
- Narratives are the primary unit for broadcast topic selection
- Narrative detection is an ongoing process, not a one-time operation

## Design

### Narrative Formation Pipeline

```
Events (from Event Engine)
    │
    ▼
┌──────────────────┐
│  Event Intake     │  Receive new/updated events
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Similarity       │  Compute pairwise event similarity
│  Computation      │
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Clustering       │  Cluster events using similarity graph
│  Algorithm        │  (Community detection on event similarity graph)
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Narrative Label  │  Generate narrative title and summary
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Scoring          │  Compute narrative strength scores
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Evolution Track  │  Detect narrative changes (growth, split, merge)
└──────┬───────────┘
       │
       ▼
┌──────────────────┐
│  Graph Write      │  Update narrative nodes in knowledge graph
└──────┬───────────┘
       │
       ▼
    ┌──────┐
    │ Emit  │  correlation.narrative.* events
    └──────┘
```

### Event Similarity Computation

Events are compared on multiple dimensions to compute a similarity score:

```rust
pub struct EventSimilarityFeatures {
    pub entity_overlap: f32,          // Jaccard similarity of entity sets
    pub temporal_proximity: f32,      // 1.0 if same day, decays over time
    pub location_overlap: f32,        // 1.0 if same location, 0 if different
    pub topic_similarity: f32,        // Embedding cosine similarity of event descriptions
    pub type_match: f32,              // 1.0 if same event_type
    pub entity_embedding_sim: f32,    // Embedding similarity of combined entity names
}

pub fn compute_event_similarity(e1: &Event, e2: &Event) -> f32 {
    let features = extract_similarity_features(e1, e2);

    // Weighted combination
    let score =
        0.30 * features.entity_overlap +
        0.20 * features.temporal_proximity +
        0.15 * features.location_overlap +
        0.20 * features.topic_similarity +
        0.10 * features.type_match +
        0.05 * features.entity_embedding_sim;

    // Apply topic boost: if same high-specificity topic, increase similarity
    let topic_boost = if is_same_domain(e1.event_type, e2.event_type) { 1.2 } else { 1.0 };

    (score * topic_boost).clamp(0.0, 1.0)
}
```

**Similarity threshold:**
- `> 0.6`: Likely same narrative
- `0.4 - 0.6`: Weak connection; may be related narratives
- `< 0.4`: Likely different narratives

### Clustering Algorithm

Narratives are formed using community detection on the event similarity graph.

**Algorithm:** Louvain community detection (or Leiden algorithm)

```rust
pub struct NarrativeClusteringConfig {
    pub similarity_threshold: f32,       // 0.5 — minimum edge weight
    pub min_events_per_narrative: u32,   // 3 — minimum to form narrative
    pub resolution: f32,                 // 1.0 — lower = fewer, larger clusters
    pub run_interval_minutes: u32,       // 30 — reclustering frequency
}
```

**Process:**
1. Build similarity graph: nodes = events, edges = weighted by similarity
2. Prune edges below similarity_threshold
3. Run community detection
4. For each community containing >= min_events_per_narrative:
   a. Create or update Narrative node
   b. Link events via EVENT_BELONGS_TO edges
5. For communities < min_events_per_narrative:
   a. Keep as unclustered events (may form narratives as new events arrive)
   b. Periodically re-check for narrative formation

**Narrative membership is soft:**
- An event can belong to up to 3 narratives (with different relevance scores)
- If an event is equidistant to two narratives, it belongs to both
- Relevance score determines primary narrative membership

### Narrative Labeling

Narratives are labeled using an LLM:

```rust
pub struct NarrativeLabel {
    pub title: String,         // "Global Supply Chain Disruptions 2026"
    pub summary: String,      // 2-3 sentence summary
    pub narrative_type: String, // "ongoing", "developing", "resolved", "recurring"
}
```

**Labeling prompt (conceptual):**

```
Given the following events, generate:
1. A concise narrative title (max 8 words)
2. A 2-3 sentence summary
3. Narrative type: "ongoing" (still developing), "developing" (just starting),
   "resolved" (concluded), "recurring" (cyclical pattern)

Events:
{event_list}

Return as JSON:
{
  "title": "...",
  "summary": "...",
  "narrative_type": "..."
}
```

### Narrative Scoring

Each narrative is scored on multiple dimensions:

```rust
pub struct NarrativeScores {
    pub strength: f32,          // Composite score (0-1)
    pub coverage_score: f32,    // How comprehensively events cover the narrative
    pub diversity_score: f32,   // Source diversity across all events
    pub recency_score: f32,     // How recently events have occurred
    pub momentum: f32,          // Rate of new event addition (positive = growing)
    pub coherence: f32,         // How tightly clustered the events are
}
```

**Score computation:**

```rust
pub fn compute_narrative_scores(narrative: &Narrative, events: &[Event]) -> NarrativeScores {
    let coverage = (narrative.event_count as f32 / 20.0).min(1.0);  // Saturates at 20 events
    let diversity = mean_source_diversity(events);
    let recency = compute_recency_score(events);
    let momentum = compute_event_addition_rate(narrative);
    let coherence = compute_internal_coherence(events);
    let strength = 0.3 * coverage + 0.25 * diversity + 0.2 * recency + 0.15 * momentum + 0.1 * coherence;

    NarrativeScores {
        strength,
        coverage_score: coverage,
        diversity_score: diversity,
        recency_score: recency,
        momentum,
        coherence,
    }
}
```

### Narrative Lifecycle

```
Forming → Active → Mature → Declining → Archived
              ↗           ↘
            Evolving    Stalled
```

| Phase | Description | Conditions |
|-------|-------------|------------|
| Forming | Initial cluster, few events | < 3 events OR < 1 hour old |
| Active | Narrative is growing | New events added in last 24h |
| Evolving | Narrative scope significantly changing | Substantially new events changing narrative nature |
| Mature | Narrative well-established | >= 5 events, stable, high coverage |
| Declining | Narrative fading | No new events in > 7 days |
| Stalled | Narrative paused | Expected new events but none in window |
| Archived | Narrative complete | No new events in > 30 days |

### Narrative Relationships

Narratives can relate to each other:

```rust
pub enum NarrativeRelationship {
    Parent,        // A is a sub-narrative of B ("US-China trade" → "Global trade tensions")
    Child,         // B is a sub-narrative of A (inverse of Parent)
    Sibling,       // A and B are sub-narratives of same parent
    Fork,          // A split into A and B (narrative branching)
    Merge,         // A and B merged into new narrative
    Related,       // A and B are loosely related
}
```

**Detection:**
- Parent/Child: If events of A are fully contained within B's topic (embedding containment)
- Fork: When an event cluster splits into two distinct clusters during reclustering
- Merge: When two event clusters merge during reclustering
- Related: High similarity score (0.4-0.6) but below merge threshold

### Periodic Maintenance

The Narrative Engine runs maintenance (default: every 30 minutes):

1. Recompute similarity graph for new/changed events
2. Incremental clustering (add new events to existing clusters)
3. Full reclustering (nightly) for global consistency
4. Score recalculation for all active narratives
5. Lifecycle transitions (Active → Declining → Archived)
6. Relationship re-evaluation (narrative forks/merges)
7. Prune narratives below minimum strength (archive very weak narratives)

### Input/Output Contracts

**Input:** Event events (from `correlation.event.*` topics)

```json
{
  "type": "correlation.event.created",
  "data": {
    "event_id": "uuid",
    "title": "Apple Q2 2026 Earnings",
    "event_type": "business",
    "importance": 0.78,
    "entities": ["entity-uuid-1", "entity-uuid-2"],
    "location": "Cupertino, CA",
    "start_time": "2026-06-02T12:00:00Z"
  }
}
```

**Output:** Narrative events (published to `correlation.narrative.*` topics)

```json
{
  "type": "correlation.narrative.updated",
  "data": {
    "narrative_id": "narrative-uuid",
    "title": "Tech Industry Financial Reports Q2 2026",
    "summary": "Major tech companies reporting quarterly earnings...",
    "narrative_type": "ongoing",
    "status": "active",
    "strength": 0.72,
    "event_count": 8,
    "source_count": 12,
    "momentum": 0.6,
    "added_event_ids": ["event-uuid-3"],
    "changed_fields": ["strength", "event_count", "momentum"]
  }
}
```

## Interfaces

- `event-engine.md` — upstream event producer
- `contradiction-engine.md` — may use narrative context for contradiction detection
- `docs/broadcast/broadcast-engine.md` — downstream consumer for topic selection
- `docs/data/knowledge-graph.md` — Narrative node and edge schemas

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Over-clustering | Unrelated events in same narrative | Adjustable resolution parameter; human split in UI |
| Under-clustering | Fragmented narratives | Lower resolution; periodic merge scan |
| Narrative labeling too generic | Uninformative titles | More detailed LLM prompt; template fallbacks |
| Event churn causes narrative thrash | Unstable narrative membership | Membership debounce; reclustering cooldown |
| Narrative drift over time | Narrative scope creeps | Periodic recalibration; event membership decay |
| Sub-narrative infinite nesting | Deep hierarchy | Max depth limit (3 levels); flatten at limit |

## Future Extensions

- Multi-narrative perspective tracking (different viewpoints on same events)
- Narrative sentiment tracking (positive/negative trajectory)
- Narrative prediction (forecast likely narrative developments)
- Causal narrative chains (event A → event B → event C)
- User-defined narrative interests (follow specific narratives)
- Narrative comparison across time periods (seasonal patterns)
