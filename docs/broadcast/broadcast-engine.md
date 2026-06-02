# Broadcast Engine

## Purpose

Define the broadcast engine — the subsystem responsible for continuously generating, scheduling, and delivering intelligence broadcasts. This is the system's primary output mechanism and must operate indefinitely without human intervention.

## Scope

This document covers the infinite operation model, broadcast scheduling, content generation, priority selection, topic selection, breaking news handling, idle behavior, and output format management. It does not cover audio generation (see `audio-system.md`).

## Responsibilities

- Operate continuously on a defined schedule
- Generate content even when no new information arrives
- Select broadcast topics from available events and narratives
- Prioritize content for maximum relevance
- Handle breaking news with rapid insertion
- Maintain content diversity across topics
- Generate multiple broadcast formats (brief, full, audio script)
- Manage broadcast queue and schedule

## Assumptions

- Broadcasts are the system's primary output; if no content is generated, the system is failing
- The system runs indefinitely; broadcast generation must never stop
- "No new information" is a valid state that must produce meaningful output
- Breaking news can arrive at any time and should interrupt normal scheduling
- Broadcast quality over quantity: fewer, higher-quality broadcasts are better
- Users configure their preferred broadcast cadence (default: every 2 hours)

## Design

### Infinite Operation Model

The broadcast engine runs on a perpetual cycle:

```
┌───────────────────────────────────────────────────────┐
│                 BROADCAST CYCLE                        │
│                                                       │
│  Wait → Collect → Rank → Generate → Deliver → Repeat  │
│    ↑                                                    │
│    └────────────────────────────────────────────────────┘
```

**Cycle phases:**

1. **Wait:** Sleep until next scheduled broadcast time (default: 2 hours)
2. **Collect:** Query knowledge graph for new/updated events, narratives, contradictions
3. **Rank:** Score and rank content by priority
4. **Generate:** Produce broadcast content in configured formats
5. **Deliver:** Write output (text report, audio file, UI update)
6. **Repeat:** Return to wait phase for next cycle

**The cycle MUST NOT stop.** Even if Collect returns zero results, the cycle continues:

- Collect returns zero → Generate produces "idle" broadcast (see Idle Behavior)
- Generate fails → Retry with exponential backoff
- Deliver fails → Queue for retry, continue to repeat
- System restart → Recover cycle state from scheduler, resume

### Broadcast Scheduling

```rust
pub struct BroadcastSchedule {
    pub cadence: Duration,             // Default: 2 hours
    pub generation_times: Vec<TimeOfDay>, // Or specific times: [06:00, 08:00, 12:00, 18:00, 22:00]
    pub max_per_day: u32,              // Maximum broadcasts per day (default: 8)
    pub min_interval: Duration,        // Minimum time between broadcasts (default: 1 hour)
    pub breaking_news: BreakingNewsConfig,
    pub timezone: String,              // User's timezone for generation times
}
```

**Schedule types:**

| Type | Description | Example |
|------|-------------|---------|
| `fixed_interval` | Broadcast every N hours | Every 2 hours |
| `fixed_times` | Broadcast at specific times | 6am, 8am, 12pm, 6pm, 10pm |
| `adaptive` | Adjust schedule based on information volume | More during active events, fewer during quiet periods |
| `manual` | User triggers broadcasts via UI | Only when requested |

### Broadcast Collection Phase

```rust
pub struct BroadcastCollection {
    pub top_events: Vec<ScoredEvent>,
    pub top_narratives: Vec<ScoredNarrative>,
    pub contradictions: Vec<ScoredContradiction>,
    pub breaking_events: Vec<ScoredEvent>,
    pub stale_narratives: Vec<ScoredNarrative>,  // Narratives with no recent updates
    pub system_health: SystemHealthSnapshot,
    pub collection_timestamp: DateTime<Utc>,
}
```

**Collect queries:**

```cypher
// Top events by importance, limited to top 20
MATCH (e:Event)
WHERE e.valid_to IS NULL AND e.status IN ['active', 'evolving']
RETURN e.id, e.title, e.importance, e.confidence, e.claim_count, e.updated_at
ORDER BY e.importance DESC
LIMIT 20

// Top narratives by strength, limited to top 10
MATCH (n:Narrative)
WHERE n.valid_to IS NULL AND n.status IN ['forming', 'active', 'mature']
RETURN n.id, n.title, n.strength, n.event_count, n.updated_at
ORDER BY n.strength DESC
LIMIT 10

// Unresolved contradictions with high severity
MATCH (c:Contradiction)
WHERE c.valid_to IS NULL AND c.resolution_status = 'unresolved' AND c.severity > 0.5
RETURN c.id, c.description, c.severity, c.created_at
ORDER BY c.severity DESC
LIMIT 5

// Breaking events (events created in last 30 minutes with high importance)
MATCH (e:Event)
WHERE e.valid_to IS NULL AND e.created_at > datetime() - duration('PT30M')
  AND e.importance > 0.6
RETURN e.id, e.title, e.importance, e.created_at
ORDER BY e.importance DESC
```

### Priority Selection

Each potential broadcast item is scored for inclusion:

```rust
pub struct PriorityScore {
    pub item_id: String,
    pub score: f32,              // Composite priority score (0-100)
    pub breakdown: PriorityBreakdown,
}

pub struct PriorityBreakdown {
    pub importance: f32,         // From event/narrative importance score (0-40)
    pub recency: f32,            // Time since last update (0-25)
    pub novelty: f32,            // Not previously broadcast (0-15)
    pub user_interest: f32,      // Matches user interest profile (0-10)
    pub diversity: f32,          // Topic diversity bonus (0-10)
}

pub fn compute_priority(item: &BroadcastItem, context: &PriorityContext) -> PriorityScore {
    // Importance: use the item's pre-computed importance
    let importance = item.importance() * 40.0;

    // Recency: recent items score higher
    let hours_since_update = (Utc::now() - item.last_updated()).num_hours() as f32;
    let recency = (1.0 / (hours_since_update + 1.0)) * 25.0;

    // Novelty: previously broadcast items get penalty
    let novelty = if context.previously_broadcast.contains(&item.id()) {
        5.0 * (1.0 - context.rebroadcast_decay)
    } else {
        15.0
    };

    // User interest: boost items matching user's followed topics
    let user_interest = compute_user_interest_match(item, &context.user_profile) * 10.0;

    // Diversity: boost underrepresented topics
    let diversity = compute_diversity_boost(item, &context.broadcast_history) * 10.0;

    let score = (importance + recency + novelty + user_interest + diversity).min(100.0);

    PriorityScore {
        item_id: item.id().to_string(),
        score,
        breakdown: PriorityBreakdown {
            importance,
            recency,
            novelty,
            user_interest,
            diversity,
        },
    }
}
```

### Topic Selection Algorithm

```rust
pub struct BroadcastContent {
    pub segments: Vec<BroadcastSegment>,
    pub metadata: BroadcastMetadata,
}

pub enum BroadcastSegment {
    TopStory {
        event: ScoredEvent,
        narrative: Option<ScoredNarrative>,
        claims: Vec<Claim>,
    },
    NarrativeUpdate {
        narrative: ScoredNarrative,
        new_events: Vec<ScoredEvent>,
    },
    ContradictionAlert {
        contradiction: ScoredContradiction,
        claims: Vec<Claim>,
    },
    IdleContent {
        reason: IdleReason,
        content: String,
    },
    SystemStatus {
        health: SystemHealthSnapshot,
    },
}
```

**Topic selection algorithm:**

1. Sort all items by priority score (descending)
2. Select top 3 stories as "Top Stories"
3. Select top 3 narrative updates (different from stories)
4. Select top 2 contradictions (if severity > 0.6)
5. Fill remaining slots with diverse topics
6. Maximum 10 segments per broadcast
7. Ensure minimum 2 segments (if idle)

**Diversity enforcement:**
- No more than 2 segments from the same domain/topic
- At least 1 segment from a different domain than previous broadcast
- If breaking news exists, it replaces the lowest-priority segment

### Breaking News Handling

Breaking news receives special handling:

```rust
pub struct BreakingNewsConfig {
    pub enabled: bool,                    // Default: true
    pub importance_threshold: f32,        // Default: 0.7
    pub max_interruptions_per_hour: u32,  // Default: 2
    pub interrupt_current_broadcast: bool, // Default: true
    pub generate_immediate: bool,         // Generate extra broadcast now
}

pub fn handle_breaking_news(event: &ScoredEvent, state: &mut BroadcastState) -> Option<Interruption> {
    if !state.config.breaking_news.enabled {
        return None;
    }

    if event.importance < state.config.breaking_news.importance_threshold {
        return None;
    }

    // Check interruption quota
    let interruptions_last_hour = state.interruptions
        .iter()
        .filter(|i| i.timestamp > Utc::now() - Duration::hours(1))
        .count();

    if interruptions_last_hour >= state.config.breaking_news.max_interruptions_per_hour {
        // Queue for next scheduled broadcast instead
        state.pending_breaking_news.push(event.clone());
        return None;
    }

    // Generate interruption
    let interruption = Interruption {
        timestamp: Utc::now(),
        event: event.clone(),
        broadcast_type: InterruptionType::BreakingNews,
    };

    state.interruptions.push(interruption.clone());
    state.pending_breaking_news.retain(|e| e.id != event.id);

    Some(interruption)
}
```

**Breaking news flow:**
1. High-importance event detected by Event Engine → `correlation.event.created` with importance > 0.7
2. Broadcast Engine receives event via subscription
3. If breaking news is enabled and quota allows:
   a. If currently generating a broadcast: flag for interruption
   b. If between broadcasts: generate immediate breaking news broadcast
4. Breaking news broadcast is a single-segment broadcast (just the breaking story)
5. Next scheduled broadcast will include the breaking news as context

### Broadcast Generation

The generation pipeline produces content from selected topics:

```rust
pub struct GenerationPipeline {
    pub steps: Vec<GenerationStep>,
}

pub enum GenerationStep {
    FactGathering,
    OutlineGeneration,
    SectionWriting,
    FactVerification,
    Formatting,
    QualityCheck,
}
```

**Step 1: Fact Gathering**
For each selected topic, query the knowledge graph for:
- Core claims (top 10 by confidence)
- Entity descriptions
- Temporal context (timeline of events)
- Source reliability information
- Supporting evidence text

**Step 2: Outline Generation**
Generate a structured outline:

```
# Top Story: [Title]
- Key facts (2-3 bullet points)
- Context (1-2 sentences)
- Key players involved
- Timeline
```

**Step 3: Section Writing**
Generate prose for each section of the outline. Each section is generated independently.

**Step 4: Fact Verification**
Verify each claim in the generated text against the source claims:

```rust
pub enum VerificationResult {
    Verified { source_id: String, confidence: f32 },
    Unverifiable { reason: String },
    Contradicted { alternative: String },
}
```

**Step 5: Formatting**
Apply broadcast format template:

| Format | Description | Length | Use Case |
|--------|-------------|--------|----------|
| `brief` | Headline + 2 sentence summary per story | 300-500 words | Quick scan |
| `full` | Full article with sections per story | 1500-3000 words | Deep read |
| `audio_script` | Conversational script for TTS | 800-2000 words | Podcast generation |
| `bullet` | Bullet-point summary | 200-300 words | Notification/alert |

**Step 6: Quality Check**

```rust
pub struct QualityCheck {
    pub minimum_segments: usize,       // Default: 2
    pub minimum_words_per_segment: usize, // Default: 50
    pub maximum_total_words: usize,    // Default: 5000
    pub require_fact_verification: bool, // Default: true
    pub require_no_hallucination: bool,   // Check claims against sources
}
```

**Generation prompt (conceptual):**

```
Generate a {format} about the following event.

Event Title: {event_title}
Key Facts:
{key_facts}

Guidelines:
- Base all statements on the provided facts
- Do not add information not in the provided facts
- Maintain neutral tone
- Include source attribution for key claims
- Write for a {audience_level} audience

Output:
```

### Idle Behavior

When no new information has been ingested or processed, the system must still generate broadcasts. Idle content includes:

**1. Summary of recent inactivity:**
```
"Since the last broadcast, no significant new information has been received.
This is expected — not every hour brings breaking news."
```

**2. Educational / context content:**
```
"During this quiet period, here is a deeper look at the ongoing
narratives being tracked: [list with brief descriptions]"
```

**3. Contradiction deep dives:**
```
"We are tracking [N] unresolved contradictions. Here is a detailed
look at the most significant one: [deep dive]"
```

**4. Entity deep dives:**
```
"Let's look at [Entity X] — here is everything we know based on
[count] sources: [detailed summary]"
```

**5. System health update:**
```
"System status: All systems operational. Tracking [X] entities,
[Y] claims, [Z] events across [W] sources."
```

**Idle content selection:**
1. If no new events or narratives: select idle content type randomly
2. Prioritize deep dives (entity, contradiction) over simple summaries
3. Ensure rotation: avoid same idle content type twice in a row
4. Include system health update every Nth idle broadcast (default: every 3rd)

```rust
pub fn generate_idle_broadcast(state: &BroadcastState) -> BroadcastContent {
    let mut segments = Vec::new();

    // Always include status summary
    segments.push(BroadcastSegment::IdleContent {
        reason: IdleReason::NoNewInformation,
        content: format!(
            "Since the last broadcast, no significant new information has been received. \
             The system is monitoring {} sources across {} topics.",
            state.source_count, state.narrative_count
        ),
    });

    // Pick a deep dive topic
    let last_idle_type = &state.last_idle_type;
    let available = IdleContentType::all()
        .filter(|t| t != last_idle_type)
        .collect::<Vec<_>>();
    let selected = available.choose(&mut rand::thread_rng());

    match selected {
        Some(IdleContentType::ContradictionDeepDive) => {
            if let Some(top_contradiction) = get_top_contradiction(state) {
                segments.push(generate_contradiction_deep_dive(top_contradiction));
            }
        }
        Some(IdleContentType::EntityDeepDive) => {
            if let Some(top_entity) = get_most_tracked_entity(state) {
                segments.push(generate_entity_deep_dive(top_entity));
            }
        }
        Some(IdleContentType::NarrativeContext) => {
            if let Some(top_narrative) = get_top_narrative(state) {
                segments.push(generate_narrative_deep_dive(top_narrative));
            }
        }
        _ => {}
    }

    BroadcastContent {
        segments,
        metadata: BroadcastMetadata {
            generated_at: Utc::now(),
            broadcast_type: BroadcastType::Idle,
            format: BroadcastFormat::Brief,
            segment_count: segments.len(),
            total_words: count_words(&segments),
        },
    }
}
```

### Broadcast Queue

```rust
pub struct BroadcastQueue {
    pub scheduled: Vec<ScheduledBroadcast>,
    pub immediate: Vec<ImmediateBroadcast>,  // Breaking news, user-triggered
}

pub struct ScheduledBroadcast {
    pub scheduled_at: DateTime<Utc>,
    pub format: BroadcastFormat,
    pub status: QueueStatus,
}

pub struct ImmediateBroadcast {
    pub priority: u32,
    pub reason: ImmediateReason,  // BreakingNews, UserRequest, SystemAlert
    pub created_at: DateTime<Utc>,
}

pub enum QueueStatus {
    Pending,
    Generating,
    Ready,
    Delivered,
    Failed { error: String, retries: u32 },
}
```

Queue processing:
1. Check scheduled broadcasts: due → move to Generating
2. Check immediate broadcasts: any → highest priority first
3. Generate broadcast content
4. Deliver (write text, generate audio, push to UI)
5. Mark as Delivered
6. Log broadcast to history

### Output Delivery

Generated broadcasts are delivered to multiple channels:

| Channel | Format | Storage | Latency |
|---------|--------|---------|---------|
| Text file | Markdown/HTML | `~/.objective/audio/broadcasts/{date}/{title}.md` | Immediate |
| Audio file | MP3/OGG | `~/.objective/audio/broadcasts/{date}/{title}.mp3` | + generation time |
| UI Dashboard | JSON via WebSocket | In-memory stream | Immediate |
| Archive | JSON metadata | Knowledge graph (Broadcast node) | Immediate |

### Broadcast History

The last N broadcasts (default: 100) are tracked for:
- Deduplication (avoid reporting same story)
- Diversity computation (ensure topic rotation)
- User review ("what did the system report this morning?")
- Quality monitoring (track empty/idle ratios)

```rust
pub struct BroadcastHistory {
    entries: Vec<BroadcastEntry>,
    max_entries: usize,  // Default: 100
}

pub struct BroadcastEntry {
    pub id: String,
    pub generated_at: DateTime<Utc>,
    pub format: BroadcastFormat,
    pub segments: Vec<String>,  // Segment IDs
    pub total_words: usize,
    pub idle: bool,
    pub breaking: bool,
    pub quality_score: f32,
}
```

## Interfaces

- `audio-system.md` — downstream audio generation from text
- `docs/api/internal-api.md` — broadcast events and state queries
- `docs/ui/dashboard-spec.md` — broadcast viewer UI
- `docs/data/knowledge-graph.md` — data sources for broadcast content
- `docs/processing/event-engine.md` — upstream event provider
- `docs/processing/narrative-engine.md` — upstream narrative provider

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| LLM unavailable for generation | No new broadcast content | Fallback to template-based broadcast (pre-written scripts with data insertion) |
| Graph query fails | No data for broadcast | Recent broadcast cache; system health broadcast |
| Generation timeout | Late broadcast | Kill and retry next cycle; reduce content complexity |
| Breaking news flood | Broadcast spam | Max interruptions per hour; queue for next scheduled |
| All content stale | Repetitive broadcasts | Idle content rotation; entity deep dives |
| Disk full for audio | Audio generation fails | Generate text-only; archive rotation |
| No sources configured | No content ever | Installation wizard requires at least one source |

## Future Extensions

- Personalized broadcasts per user (multi-user system)
- Broadcast RSS/Atom feed for external consumption
- Podcast RSS feed generation (automated podcast publishing)
- Multi-language broadcast generation
- User feedback buttons ("more like this", "less like this")
- Broadcast scheduling based on user calendar (avoid meetings)
- Collaborative broadcasts (merge insights across Objective instances)
- Broadcast sponsorship/intro customization
