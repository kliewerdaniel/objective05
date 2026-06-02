# Contradiction Engine

## Purpose

Define the contradiction engine — the subsystem responsible for detecting, classifying, scoring, and tracking contradictions between claims in the knowledge graph.

## Scope

This document covers contradiction types, detection algorithms, confidence scoring, resolution strategies, and contradiction lifecycle management.

## Responsibilities

- Detect contradictory claims about the same subject
- Classify contradiction types (direct, implied, temporal, factual error)
- Score contradiction confidence based on claim strength and contradiction severity
- Suggest resolution strategies
- Track resolved contradictions
- Emit contradiction events for UI and broadcast awareness

## Assumptions

- Contradictions exist between claims, not between events or narratives directly
- Contradictions can be resolved by new evidence, source hierarchy, or human judgment
- Some apparent contradictions may be false (claims about different time periods, contexts, or entities)
- Contradiction detection is computationally expensive; run on a schedule, not real-time
- The system does not automatically resolve contradictions — it surfaces them for attention

## Design

### Contradiction Types

| Type | Description | Example |
|------|-------------|---------|
| `direct` | Same subject, predicate, contradictory objects | "Revenue was $10B" vs "Revenue was $12B" |
| `implied` | Claims imply contradictory conclusions | "Company is profitable" vs "Company reported losses" |
| `temporal` | Same subject, different times, but claims are incompatible as stated | "CEO is stepping down" (Jan) vs "CEO committed to 5 more years" (Feb) |
| `factual_error` | One claim contradicts established fact | "Population is 500M" vs established population of 340M |
| `source_attribution` | Source A says X, source B says not-X | "Official: no layoffs planned" vs "Source: layoffs imminent" |
| `quantitative` | Numerical inconsistency | "Spending increased 10%" vs "Spending increased 50%" |

### Detection Pipeline

```
New Claims (or periodic scan)
    │
    ▼
┌──────────────────────┐
│  Candidate Generation  │  Find pairs of claims that might contradict
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Contradiction Check   │  Apply detection algorithms to candidate pairs
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Confidence Scoring   │  Score the contradiction confidence
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Classification       │  Categorize and describe the contradiction
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Deduplication        │  Check if already known contradiction
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Graph Write          │  Create/update Contradiction node
└──────┬───────────────┘
       │
       ▼
    ┌──────┐
    │ Emit  │  correlation.contradiction.*
    └──────┘
```

### Candidate Generation

Contradiction detection is O(n²) in the claim space. To manage complexity, candidates are generated using targeted queries rather than full pairwise comparison.

```rust
pub fn generate_contradiction_candidates(claims: &[Claim]) -> Vec<(ClaimId, ClaimId)> {
    // Strategy 1: Same subject, same predicate, different object
    // "Revenue was $10B" vs "Revenue was $12B"
    let same_subject_predicate = claims
        .group_by(|c| (c.subject_id, c.predicate))
        .filter(|(_, group)| group.len() > 1)
        .flat_map(|(_, group)| pairwise(group))
        .collect();

    // Strategy 2: Same subject, contradictory predicates
    // "Company is profitable" vs "Company reported losses"
    let contradictory_predicates = claims
        .group_by(|c| c.subject_id)
        .filter(|(_, group)| has_contradictory_predicates(group))
        .flat_map(|(_, group)| find_contradictory_pairs(group))
        .collect();

    // Strategy 3: Claims about same entity with significantly different quantitative values
    // "GDP grew 3%" vs "GDP grew 10%"
    let quantitative_conflicts = claims
        .filter(is_quantitative)
        .group_by(|c| (c.subject_id, c.predicate))
        .filter(|(_, group)| has_significant_value_difference(group))
        .flat_map(|(_, group)| pairwise(group))
        .collect();

    // De-duplicate and return
    let all: Vec<_> = same_subject_predicate
        .into_iter()
        .chain(contradictory_predicates)
        .chain(quantitative_conflicts)
        .unique()
        .collect();

    // Limit to top N most promising candidates for computational efficiency
    all.into_iter().take(MAX_CANDIDATES).collect()
}
```

### Detection Algorithms

#### Direct Contradiction Detection

```rust
pub fn check_direct_contradiction(c1: &Claim, c2: &Claim) -> Option<ContradictionResult> {
    // Requires: same subject, same predicate
    if c1.subject_id != c2.subject_id || c1.predicate != c2.predicate {
        return None;
    }

    let object_eq = claims_have_equal_object(c1, c2);
    let value_diff = claims_have_significantly_different_value(c1, c2);

    if object_eq {
        return None; // Same claim, not a contradiction
    }

    if value_diff {
        return Some(ContradictionResult {
            contradiction_type: if is_numeric(c1) { "quantitative" } else { "direct" },
            severity: compute_severity(c1, c2),
            description: format!("{} vs {}", c1.claim_text, c2.claim_text),
        });
    }

    // Different object values — could be contradictory or just different aspects
    // Send to LLM for evaluation if above confidence threshold
    if (c1.confidence + c2.confidence) / 2.0 > 0.7 {
        return evaluate_with_llm(c1, c2);
    }

    None
}
```

#### Implied Contradiction Detection

```rust
pub fn check_implied_contradiction(c1: &Claim, c2: &Claim) -> Option<ContradictionResult> {
    // Send pair to LLM with context to determine if claims imply contradiction
    // Only for high-confidence claims about the same entity
    if c1.subject_id != c2.subject_id {
        return None;
    }

    if c1.confidence < 0.6 || c2.confidence < 0.6 {
        return None;
    }

    evaluate_implied_contradiction_with_llm(c1, c2)
}
```

#### Temporal Contradiction Detection

```rust
pub fn check_temporal_contradiction(c1: &Claim, c2: &Claim) -> Option<ContradictionResult> {
    // Same subject, claims about different time periods
    // But the claims should be compatible across time
    // If not, it's a temporal contradiction

    let t1 = get_temporal_context(c1);
    let t2 = get_temporal_context(c2);

    if t1.is_none() || t2.is_none() {
        return None;
    }

    let (t1_start, t1_end) = t1.unwrap();
    let (t2_start, t2_end) = t2.unwrap();

    // Time periods don't overlap — claims are about different times
    // This is NOT a contradiction unless the claims are about ongoing states
    if t1_end < t2_start || t2_end < t1_start {
        // Claims about different times are generally not contradictory
        // UNLESS both claim permanent/ongoing states
        if is_ongoing_state(c1) && is_ongoing_state(c2) {
            return evaluate_temporal_conflict(c1, c2, t1, t2);
        }
        return None;
    }

    // Time periods overlap — claims should be compatible
    // Send to LLM for evaluation
    evaluate_temporal_contradiction_with_llm(c1, c2)
}
```

### LLM-Based Evaluation

For complex contradiction types (implied, temporal, attribution), an LLM evaluates the candidate pair:

**Prompt (conceptual):**

```
Determine if these two claims contradict each other.

Claim A: "{claim_a}" (Source: {source_a}, Time: {time_a})
Claim B: "{claim_b}" (Source: {source_b}, Time: {time_b})

Context (shared subject): {entity_context}

Does Claim B contradict Claim A?
Options:
- YES_DIRECT: Direct contradiction (same topic, opposite facts)
- YES_IMPLIED: Implied contradiction (logically incompatible)
- YES_TEMPORAL: Temporal contradiction (time-incompatible)
- NO_COMPATIBLE: Claims are compatible (different aspects)
- NO_DIFFERENT_TOPIC: Claims about different things
- UNCLEAR: Cannot determine from available information

If YES, provide severity (0.0-1.0) and brief explanation.
```

### Confidence Scoring

```rust
pub struct ContradictionScore {
    pub confidence: f32,        // How confident we are this is a real contradiction
    pub severity: f32,          // How significant the contradiction is
    pub claim_a_confidence: f32,
    pub claim_b_confidence: f32,
    pub source_reliability_diff: f32, // Difference in source trustworthiness
}

pub fn score_contradiction(
    c1: &Claim,
    c2: &Claim,
    detection_result: &ContradictionResult,
) -> ContradictionScore {
    // Base confidence: strength of the detection signal
    let base_confidence = match detection_result.detection_method {
        DetectionMethod::ExactMatch => 0.9,
        DetectionMethod::QuantitativeAnalysis => 0.8,
        DetectionMethod::LLMEvaluation => 0.7,
        DetectionMethod::ImpliedReasoning => 0.5,
    };

    // Adjust by claim confidences
    let claim_factor = (c1.confidence * c2.confidence).sqrt();
    let confidence = base_confidence * claim_factor;

    // Severity: how important is this contradiction?
    let severity = (
        0.4 * detection_result.semantic_severity +
        0.3 * claim_factor +
        0.2 * (c1.importance + c2.importance) / 2.0 +
        0.1 * (c1.mention_count + c2.mention_count).min(100) as f32 / 100.0
    ).min(1.0);

    ContradictionScore {
        confidence,
        severity,
        claim_a_confidence: c1.confidence,
        claim_b_confidence: c2.confidence,
        source_reliability_diff: (c1.source_reliability - c2.source_reliability).abs(),
    }
}
```

### Resolution Strategies

Contradictions can be resolved through several strategies:

| Strategy | Description | When Applied |
|----------|-------------|-------------|
| `source_hierarchy` | Higher-reliability source wins | Source reliability difference > 0.3 |
| `temporal_resolution` | Newer information supersedes older | Clear temporal ordering |
| `contextual_resolution` | Claims were about different contexts | LLM determines no actual contradiction |
| `evidence_accumulation` | More evidence supports one claim | > 3:1 evidence ratio |
| `human_judgment` | User manually resolves | UI-based resolution |
| `awaiting_information` | Cannot resolve yet; mark for follow-up | Default for uncertain contradictions |

```rust
pub fn suggest_resolution(c1: &Claim, c2: &Claim) -> ResolutionSuggestion {
    let reliability_diff = c1.source_reliability - c2.source_reliability;

    if reliability_diff.abs() > 0.3 {
        let winner = if reliability_diff > 0.0 { &c1 } else { &c2 };
        return ResolutionSuggestion {
            strategy: "source_hierarchy",
            description: format!("{} is from higher-reliability source", winner.claim_text),
            automated: true,  // Can auto-resolve
        };
    }

    if has_temporal_order(c1, c2) {
        let newer = whichever_is_newer(c1, c2);
        return ResolutionSuggestion {
            strategy: "temporal_resolution",
            description: format!("{} is more recent", newer.claim_text),
            automated: false,  // Flag for review; temporal may not invalidate
        };
    }

    ResolutionSuggestion {
        strategy: "awaiting_information",
        description: "Insufficient evidence to resolve automatically".to_string(),
        automated: false,
    }
}
```

### Contradiction Lifecycle

```
Detected → Confirmed → Monitoring → Resolved → Archived
               ↘              ↗
              False Positive (dismissed)
```

| Phase | Description |
|-------|-------------|
| Detected | Initial detection, pending confirmation |
| Confirmed | Passed secondary verification (re-check with context) |
| Monitoring | Watching for new evidence that might resolve |
| Resolved | One claim invalidated or context clarified |
| False Positive | Determined not to be a real contradiction |
| Archived | No longer relevant (claims expired) |

### Periodic Maintenance

The Contradiction Engine runs maintenance (default: every 60 minutes):

1. Scan new high-confidence claims for contradictions
2. Re-check unresolved contradictions with new evidence
3. Auto-resolve contradictions with clear source hierarchy
4. Archive old resolved contradictions
5. Re-evaluate false positives if new evidence emerges

### Input/Output Contracts

**Input:** Claim events (from `extraction.claim.*` topics)

```json
{
  "type": "extraction.claim.extracted",
  "data": {
    "claim_id": "uuid",
    "claim_text": "Revenue was $10 billion",
    "subject_id": "entity-uuid",
    "predicate": "reported_revenue",
    "object_value": "$10 billion",
    "confidence": 0.9,
    "source_document_id": "doc-uuid"
  }
}
```

**Output:** Contradiction events (published to `correlation.contradiction.*` topics)

```json
{
  "type": "correlation.contradiction.detected",
  "data": {
    "contradiction_id": "contra-uuid",
    "contradiction_type": "direct",
    "description": "\"Revenue was $10B\" vs \"Revenue was $12B\"",
    "severity": 0.7,
    "confidence": 0.85,
    "claim_ids": ["claim-uuid-1", "claim-uuid-2"],
    "entity_id": "entity-uuid",
    "resolution_status": "unresolved",
    "suggested_resolution": {
      "strategy": "temporal_resolution",
      "description": "Claims are from different quarters"
    }
  }
}
```

## Interfaces

- `extraction-engine.md` — upstream claim producer
- `docs/data/knowledge-graph.md` — Contradiction node schema
- `docs/broadcast/broadcast-engine.md` — contradictions are broadcast-relevant content
- `docs/ui/dashboard-spec.md` — contradiction viewer in UI

## Failure Modes

| Failure | Impact | Mitigation |
|--------|--------|------------|
| False positive contradiction | Noise in graph | Confidence threshold; false positive feedback loop |
| False negative (miss) | Unknown contradiction | Improve candidate generation; LLM re-evaluation |
| LLM hallucination in evaluation | Wrong classification | Conservative thresholds; multiple evaluations for borderline cases |
| Computation too expensive | Long maintenance cycles | Tiered detection (cheap checks first, expensive LLM last) |
| Circular resolution | Never resolves | Max cycles before requiring human input |
| Source reliability misconfigured | Wrong auto-resolution | Validate source reliability config; allow human override |

## Future Extensions

- Cross-narrative contradiction detection (narratives that contradict each other)
- Contradiction trend tracking (are contradictions increasing or decreasing?)
- Automated fact-checking integration (external APIs for verification)
- Claim retraction tracking (source retracts a claim → propagate to contradictions)
- Contradiction severity prediction (will this contradiction matter in a week?)
- Multi-claim contradiction networks (3+ claims forming complex contradiction patterns)
