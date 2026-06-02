# Extraction Engine

## Purpose

Define the extraction engine — the subsystem responsible for transforming raw documents into structured knowledge graph entities, claims, and relationships.

## Scope

This document covers entity extraction, claim extraction, relationship extraction, event extraction from text, chunking strategies, deduplication, and the extraction pipeline. It does not cover downstream correlation (see `event-engine.md`, `narrative-engine.md`, `contradiction-engine.md`).

## Responsibilities

- Extract named entities from document text
- Extract factual claims from document text
- Extract relationships between entities
- Extract event indicators (temporal, locative, participant)
- Deduplicate against existing knowledge graph
- Merge new extractions into the knowledge graph
- Emit extraction events for downstream processing

## Assumptions

- Documents have already been normalized by the ingestion pipeline
- Documents are primarily text (HTML stripped, plain text or markdown)
- Extraction quality is probabilistic, not deterministic
- Entity recognition may have false positives
- Claim extraction requires an LLM (not purely pattern-based)
- The extraction model runs locally via llama.cpp

## Design

### Pipeline Overview

```
Raw Document
    │
    ▼
┌──────────────┐
│   Chunker    │  Split into manageable chunks (max 4096 tokens)
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Entity       │  Extract named entities with NER model
│  Extraction   │  Returns: [Entity {type, name, aliases, metadata}]
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Claim        │  Extract factual statements
│  Extraction   │  Returns: [Claim {subject, predicate, object, evidence}]
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Relationship  │  Extract inter-entity relationships
│  Extraction   │  Returns: [Relationship {type, from, to, evidence}]
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Event        │  Extract event indicators
│  Indicators   │  Returns: [EventIndicator {type, time, location, participants}]
└──────┬───────┘
       │
       ▼
┌──────────────┐
│  Dedup &      │  Match against existing graph, merge or create
│  Merge        │  Updates: Entity, Claim, Relationship nodes
└──────┬───────┘
       │
       ▼
    ┌──────┐
    │ Graph │  Write to knowledge graph
    │ Write │  Emit extraction.* events
    └──────┘
```

### Document Chunking

Documents are split into chunks that fit within the model's context window.

```rust
pub struct ChunkingConfig {
    pub max_tokens: usize,           // Default: 4096
    pub overlap_tokens: usize,       // Default: 128 (overlap between chunks)
    pub preserve_boundaries: bool,   // Default: true (don't split mid-sentence)
}
```

**Chunking strategy:**
1. If document length < max_tokens: single chunk
2. Otherwise: split at paragraph boundaries
3. If paragraph exceeds max_tokens: split at sentence boundaries
4. If sentence exceeds max_tokens: split at token boundary (last resort)
5. Each chunk includes 128-token overlap with previous chunk for context
6. Overlapping regions are deduplicated during merge

### Entity Extraction

```rust
pub struct ExtractedEntity {
    pub name: String,
    pub entity_type: EntityType,     // Person, Organization, Location, Concept, Event_Topic
    pub aliases: Vec<String>,
    pub description: Option<String>,
    pub metadata: HashMap<String, Value>,
    pub confidence: f32,             // 0.0 - 1.0
    pub evidence_snippet: String,    // Text that supports this extraction
    pub position: TextRange,         // Character offset in document
}
```

**Extraction prompt (conceptual):**

```
Extract all named entities from the following text.
For each entity, provide:
- The entity name (canonical form)
- Entity type (Person, Organization, Location, Concept, Event_Topic)
- Any aliases or alternate names mentioned
- A brief description (if discernible from context)

Text:
{chunk}

Return as JSON array:
[
  {
    "name": "...",
    "type": "Person",
    "aliases": ["..."],
    "description": "...",
    "confidence": 0.95
  }
]
```

**Entity type classification:**

| Type | Examples | Extraction Signals |
|------|----------|-------------------|
| Person | "John Smith", "President Biden" | Titles, names, pronouns |
| Organization | "Apple Inc", "United Nations" | Legal suffixes, acronyms |
| Location | "New York", "European Union" | Geography, prepositions |
| Concept | "Quantum Computing", "Inflation" | Abstract nouns, technical terms |
| Event_Topic | "World Cup 2026", "COP30" | Dates, event keywords |

**Post-processing:**
1. Filter low-confidence entities (remove < 0.3 confidence)
2. Normalize entity names (strip whitespace, standardize capitalization)
3. Resolve entity type conflicts (if same entity extracted as different types, keep higher confidence)
4. Merge aliases for entities mentioned multiple times in document

### Claim Extraction

```rust
pub struct ExtractedClaim {
    pub claim_text: String,            // The full claim as stated
    pub subject_name: String,          // Entity name of subject
    pub predicate: String,             // Relationship predicate
    pub object_name: Option<String>,   // Entity name of object (if entity)
    pub object_value: Option<String>,  // Literal value (if not entity)
    pub claim_type: ClaimType,         // attribution, relation, quantification, temporal
    pub sentiment: Option<f32>,        // -1.0 to 1.0
    pub confidence: f32,               // 0.0 - 1.0
    pub evidence_snippet: String,      // Exact text supporting the claim
    pub attributed_to: Option<String>, // Speaker/author if attributed
}
```

**Claim types:**

| Type | Description | Example |
|------|-------------|---------|
| `attribution` | X said Y | "The CEO stated revenue grew 10%" |
| `relation` | X is related to Y | "Apple is based in Cupertino" |
| `quantification` | X has value Y | "GDP grew by 3.2%" |
| `temporal` | X happened at time Y | "The merger closed in June 2026" |
| `comparison` | X compared to Y | "Sales exceeded expectations" |

**Extraction prompt (conceptual):**

```
Extract factual claims from the following text.
For each claim, identify:
- The claim text (exact or paraphrased)
- The subject entity
- The predicate/relationship
- The object (entity or literal value)
- Claim type
- Sentiment (-1 to 1)
- Who is making the claim (if attributed)

Text:
{chunk}

Return as JSON:
[
  {
    "claim_text": "Revenue grew 10% in Q2 2026",
    "subject_name": "Company X",
    "predicate": "revenue_growth",
    "object_value": "10%",
    "claim_type": "quantification",
    "sentiment": 0.3,
    "confidence": 0.9,
    "evidence_snippet": "revenue grew 10% in Q2 2026",
    "attributed_to": "CEO John Smith"
  }
]
```

**Post-processing:**
1. Filter low-confidence claims (remove < 0.4)
2. Normalize predicates (map to canonical forms via synonym table)
3. Resolve subject/object to known entities (link by name)
4. If subject/object not in extracted entities, create placeholder entity
5. Deduplicate claims within document (same subject + predicate + object)

### Relationship Extraction

```rust
pub struct ExtractedRelationship {
    pub from_entity_name: String,
    pub to_entity_name: String,
    pub relationship_type: String,  // works_for, located_in, part_of, founded_by, etc.
    pub confidence: f32,
    pub evidence_snippet: String,
}
```

**Relationship types:**

| Type | Inverse | Description |
|------|---------|-------------|
| `works_for` | `employs` | Person works for Organization |
| `located_in` | `contains` | Entity located in Location |
| `part_of` | `has_part` | Entity is part of larger entity |
| `founded_by` | `founded` | Organization founded by Person |
| `acquired` | `acquired_by` | Organization acquired another |
| `invested_in` | `investor` | Organization invested in another |
| `collaborates_with` | `collaborates_with` | Mutual collaboration |
| `opposes` | `opposed_by` | Active opposition |
| `supports` | `supported_by` | Active support |
| `leads` | `led_by` | Person leads Organization |

**Post-processing:**
1. Filter < 0.5 confidence relationships
2. Remove self-relationships (from == to)
3. Verify both entities exist in extraction
4. Map to canonical relationship type names

### Event Indicator Extraction

```rust
pub struct EventIndicator {
    pub event_type: String,              // political, business, disaster, etc.
    pub key_entities: Vec<String>,        // Entity names involved
    pub location: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub description: String,
    pub confidence: f32,
    pub evidence_snippet: String,
}
```

Event indicators are more lightweight than full events. They signal that something happened and provide enough context for the Event Engine to form proper events.

**Extraction prompt (conceptual):**

```
Does this text describe a specific event? If yes, extract:
- Event type
- Key entities/participants
- Location
- Time (when did it happen?)
- Brief description

Text:
{chunk}

Return null if no clear event is described.
```

### Deduplication & Merge

Before writing to the knowledge graph, all extractions are checked against existing data.

```rust
pub struct DedupConfig {
    pub entity_name_similarity_threshold: f32,     // 0.85 — fuzzy match entity names
    pub claim_exact_match_threshold: f32,           // 0.95 — exact subject/predicate/object
    pub relationship_match_threshold: f32,           // 0.9 — from/type/to match
}
```

**Entity deduplication:**
1. Search for entity by canonical name (exact)
2. Search by alias (contains)
3. Fuzzy match name (Levenshtein distance < 2 or substring match)
4. If match found: merge aliases, increment mention_count, update confidence
5. If no match: create new entity node

**Claim deduplication:**
1. Match by subject + predicate + object (canonicalized)
2. If match found: add source document to provenance, update confidence
3. If no match: create new claim node

**Merge rules (see ADR-007):**
- Timestamp priority: newer information wins on conflict
- Confidence tiebreaker: higher confidence wins on equal timestamp
- Both versions retained via versioning (see `provenance-model.md`)

### Input/Output Contracts

**Input:** RawDocument (from document store)

```json
{
  "id": "uuid",
  "source_id": "uuid",
  "title": "...",
  "body": "full text...",
  "published_at": "2026-06-02T11:00:00Z",
  ...
}
```

**Output:** Extraction Result (published as batch)

```json
{
  "document_id": "uuid",
  "entities": [
    {
      "name": "Apple Inc",
      "entity_type": "Organization",
      "confidence": 0.95,
      "aliases": ["Apple"],
      "description": "Technology company based in Cupertino, CA"
    }
  ],
  "claims": [
    {
      "claim_text": "Apple reported quarterly revenue of $94.8 billion",
      "subject_name": "Apple Inc",
      "predicate": "reported_revenue",
      "object_value": "$94.8 billion",
      "claim_type": "quantification",
      "confidence": 0.92
    }
  ],
  "relationships": [
    {
      "from_entity_name": "Apple Inc",
      "to_entity_name": "Cupertino",
      "relationship_type": "located_in",
      "confidence": 0.85
    }
  ],
  "event_indicators": []
}
```

**Events emitted:**
- `extraction.entity.extracted` — per new/updated entity
- `extraction.claim.extracted` — per new/updated claim
- `extraction.relationship.formed` — per new relationship
- `extraction.document.processed` — when document extraction is complete

### Performance Requirements

| Operation | Target | Degradation Threshold |
|-----------|--------|----------------------|
| Per-document extraction | < 30 seconds | > 60 seconds |
| Entity extraction per 1K tokens | < 5 seconds | > 15 seconds |
| Claim extraction per 1K tokens | < 10 seconds | > 30 seconds |
| Dedup query per batch | < 2 seconds | > 5 seconds |
| Concurrent extraction slots | 2 | Memory-bound |

## Interfaces

- `event-engine.md` — downstream consumer of extraction events
- `narrative-engine.md` — downstream consumer
- `contradiction-engine.md` — downstream consumer
- `docs/data/knowledge-graph.md` — graph schema for extraction output
- `docs/ai/model-strategy.md` — model selection for extraction tasks

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| LLM returns malformed JSON | Extraction fails for chunk | Retry with stricter prompt, fallback regex parser |
| LLM hallucinates entities | False entities in graph | Confidence threshold filtering, source attribution |
| Infinite loop on large doc | Resource exhaustion | Token limit, chunk size cap, timeout per document |
| Embedding model unavailable | Dedup fallback | Fall back to exact match only |
| Extraction too slow | Backpressure on ingestion | Reduce concurrent extraction slots, queue growth alert |
| Entity name collision | Wrong entity merged | Humans can disambiguate via UI; confidence-weighted votes |

## Future Extensions

- Multi-modal extraction (image, audio, video)
- Cross-document entity resolution (beyond name matching)
- Active learning: user corrections improve extraction
- Incremental extraction (extract only new/changed content)
- Custom extraction pipelines per source type
- Human-in-the-loop extraction verification
