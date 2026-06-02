# Roadmap

## Purpose

Define the development roadmap for Objective, organized into phases with priorities, dependencies, and deliverables.

## Scope

This document covers Phase 1 through Phase 5, including priorities, dependencies, milestones, and success criteria for each phase.

## Responsibilities

- Define phased development sequence
- Specify priorities within each phase
- Document dependencies between phases
- Define success criteria for each phase
- Guide resource allocation and planning

## Assumptions

- The roadmap is a living document, updated as development progresses
- Phases may overlap (Phase 2 work can start before Phase 1 fully completes)
- Priorities reflect user value, dependency constraints, and risk mitigation
- Community contributions may accelerate or reprioritize items

## Design

### Phase Overview

| Phase | Name | Focus | Duration | Team | Users |
|-------|------|-------|----------|------|-------|
| 1 | Foundation | Core plumbing, single source, basic broadcast | 3-4 months | Core team | Developer preview |
| 2 | Intelligence | Events, narratives, contradictions | 2-3 months | Core team + ML | Alpha testers |
| 3 | Quality | Multiple sources, multi-modal, audio, dashboard | 2-3 months | Core team + FE | Beta testers |
| 4 | Scale | Plugin system, performance, reliability | 2-3 months | Core team + community | General availability |
| 5 | Autonomy | Goal-oriented operation, deep dives, ecosystem | Ongoing | Community | Stable |

### Dependency Graph

```
Phase 1 ──────────────────────────────────────────────────────────────
  ├── Core types + config
  ├── Kuzu graph integration
  ├── LLM runtime (llama.cpp)
  ├── Embedding runtime (ONNX)
  ├── Message bus (NATS)
  ├── Single source adapter (RSS)
  ├── Basic entity/claim extraction
  ├── Basic knowledge graph storage
  └── Simple text broadcast output
       │
       ▼
Phase 2 ──────────────────────────────────────────────────────────────
  ├── Event engine
  ├── Narrative engine
  ├── Contradiction engine
  ├── Multiple source types
  ├── Document store + dedup
  ├── Provenance model
  ├── Temporal queries
  └── Confidence tracking
       │
       ▼
Phase 3 ──────────────────────────────────────────────────────────────
  ├── Audio system (TTS + podcast)
  ├── Dashboard (React UI)
  ├── API gateway (REST + WebSocket)
  ├── Breaking news handling
  ├── Idle broadcast content
  ├── Broadcast scheduling
  └── Basic operations (backup, restore)
       │
       ▼
Phase 4 ──────────────────────────────────────────────────────────────
  ├── Plugin API (gRPC)
  ├── Plugin host
  ├── Performance optimization
  ├── Reliability testing
  ├── Installation packages (all platforms)
  ├── Observability (metrics, logging, tracing)
  └── Documentation complete
       │
       ▼
Phase 5 ──────────────────────────────────────────────────────────────
  ├── Goal-oriented operation
  ├── Deep dive agents
  ├── Entity/event/contradiction deep dives
  ├── Plugin marketplace
  ├── Community contributions
  ├── Multi-instance sync (peer-to-peer)
  └── Continuous improvement
```

---

### Phase 1: Foundation

**Goal:** A working system that ingests a single source (RSS), extracts basic entities and claims, stores them in a knowledge graph, and generates a simple text broadcast.

**Duration:** 3-4 months

**Team:** 2-3 backend engineers, 1 ML engineer

**Deliverables:**

| Priority | Feature | Description | Dependencies |
|----------|---------|-------------|--------------|
| P0 | Core types & traits | Entity, Claim, Document, Source data types | None |
| P0 | Configuration system | YAML-based config loading, validation | None |
| P0 | Kuzu graph integration | Database connection, schema creation, basic CRUD | Core types |
| P0 | LLM runtime (llama.cpp) | Model loading, inference, queue, FFI bindings | None |
| P0 | Embedding runtime (ONNX) | Model loading, embedding generation | None |
| P0 | Message bus (NATS) | Embedded NATS, topic definitions, event publishing | None |
| P1 | RSS source adapter | Fetch, parse, normalize RSS/Atom feeds | Core types, Config |
| P1 | Basic extraction pipeline | Entity + claim extraction via LLM | LLM runtime, Chunker |
| P1 | Knowledge graph write | Store entities, claims with provenance | Graph, Extraction |
| P1 | Basic broadcast (text) | Simple text report from recent claims | Graph queries |
| P1 | Scheduler | Periodic job triggering | Message bus |
| P2 | Health service | Service monitoring, heartbeat, restart | Message bus |
| P2 | CLI commands | `start`, `stop`, `status`, `setup` | App lifecycle |

**Success criteria:**
- [ ] System ingests an RSS feed and stores raw documents
- [ ] Entities and claims extracted with > 70% reasonable precision
- [ ] Knowledge graph stores entities, claims, relationships
- [ ] Text broadcast generated every 2 hours
- [ ] System runs for 7 days without crashing
- [ ] All services recover from crash within 30 seconds
- [ ] `objective start`, `objective status`, `objective stop` work

**Known risks:**
- llama.cpp FFI bindings may have memory safety issues → thorough testing
- Kuzu Rust bindings may be immature → fallback to C API
- LLM extraction quality may be poor → iterate on prompts
- Single-machine resource contention → monitor, adjust concurrency

---

### Phase 2: Intelligence

**Goal:** Add event formation, narrative detection, contradiction detection. Support multiple source types. Implement provenance and confidence tracking.

**Duration:** 2-3 months

**Team:** 2 backend engineers, 1 ML engineer

**Deliverables:**

| Priority | Feature | Description | Dependencies |
|----------|---------|-------------|--------------|
| P0 | Event engine | Group claims into events, score importance | Phase 1 graph |
| P0 | Narrative engine | Cluster events into narratives, score strength | Event engine |
| P0 | Contradiction engine | Detect contraditions between claims | Phase 1 claims |
| P1 | Additional source types | Reddit, YouTube, PDF, Blog, SEC, Hacker News | Phase 1 RSS adapter |
| P1 | Document store | Raw document persistence, compression, indexing | Storage |
| P1 | Deduplication | URL, content hash, similarity-based dedup | Document store |
| P1 | Provenance model | Source attribution, versioning, audit trail | Graph schema |
| P1 | Temporal queries | Point-in-time, time-range, history queries | Graph |
| P1 | Confidence tracking | Score computation, decay, update | Provenance |
| P2 | Embedding service | Entity/document embedding, similarity search | ONNX runtime |
| P2 | Vector store (LanceDB) | Embedding storage, similarity queries | Embedding service |
| P2 | Source health monitoring | Error tracking, auto-disable, alerting | Ingestion |

**Success criteria:**
- [ ] Events formed from related claims with > 80% reasonable grouping
- [ ] Narratives formed from related events
- [ ] Contradictions detected between conflicting claims
- [ ] 6+ source types supported
- [ ] Provenance tracked for all graph nodes
- [ ] Temporal queries return correct point-in-time state
- [ ] Confidence scores reasonably reflect evidence quality
- [ ] System runs for 14 days without data corruption

**Known risks:**
- Narrative clustering quality depends heavily on embedding quality → iterate
- Contradiction detection has high false positive rate → tuning needed
- Temporal queries may be slow on Kuzu without proper indexes → index tuning
- Multiple source types increase maintenance burden → abstract adapter pattern

---

### Phase 3: Quality

**Goal:** Audio output (TTS + podcast), dashboard UI, breaking news, idle content, operational tooling.

**Duration:** 2-3 months

**Team:** 1 backend engineer, 1 ML engineer, 1 frontend engineer

**Deliverables:**

| Priority | Feature | Description | Dependencies |
|----------|---------|-------------|--------------|
| P0 | Audio system (TTS) | Piper TTS integration, voice management | Phase 1 LLM runtime |
| P0 | Podcast generation | Multi-voice, intro/outro, transitions | TTS, Broadcast |
| P0 | Dashboard (React) | UI shell, navigation, feed page | API gateway |
| P0 | API gateway (REST) | Event, narrative, entity endpoints | All Phase 2 services |
| P1 | API gateway (WebSocket) | Real-time event streaming | API gateway |
| P1 | Breaking news | High-importance event detection, immediate broadcast | Event engine |
| P1 | Idle broadcast content | Entity deep dives, system status, educational content | Broadcast engine |
| P1 | Broadcast scheduling | Multiple schedule types, queue management | Broadcast engine |
| P1 | Dashboard pages | Events, Narratives, Graph, Broadcasts, Sources, Settings | API gateway |
| P2 | Backup & restore | Snapshot creation, listing, restore | Storage |
| P2 | Operations CLI | `snapshot`, `db`, `queue`, `storage` subcommands | Core |
| P2 | Data export | JSON, CSV, RDF export | Graph, Document store |

**Success criteria:**
- [ ] Audio broadcast generated with multi-voice TTS
- [ ] Dashboard displays feed, events, narratives, graph in real-time
- [ ] Breaking news generates immediate broadcast
- [ ] Idle broadcasts generated when no new information
- [ ] System produces at least one broadcast format continuously
- [ ] Backup and restore works correctly
- [ ] Data export produces valid, usable files
- [ ] 90% of dashboard interactions complete within 200ms

**Known risks:**
- Piper TTS quality may not meet user expectations → plan Coqui upgrade path
- Graph visualization performance with large graphs → WebGL rendering
- Audio storage may accumulate quickly → retention enforcement
- Dashboard dev effort may be underestimated → prioritize core pages first

---

### Phase 4: Scale

**Goal:** Plugin system, performance optimization, reliability hardening, full platform distribution, observability.

**Duration:** 2-3 months

**Team:** 2 backend engineers, 1 ML engineer, 1 frontend engineer, 1 DevOps

**Deliverables:**

| Priority | Feature | Description | Dependencies |
|----------|---------|-------------|--------------|
| P0 | Plugin API | gRPC contract, protobuf definitions, SDK | Phase 2 services |
| P0 | Plugin host | Discovery, lifecycle, process management, sandboxing | Plugin API |
| P0 | Performance optimization | Query optimization, caching, concurrency tuning | All |
| P1 | Reliability testing | Chaos engineering, fault injection, recovery testing | All |
| P1 | Installation packages | brew, apt, winget, Docker | Build pipeline |
| P1 | Logging infrastructure | Structured logs, rotation, query | Core |
| P1 | Metrics collection | Prometheus endpoint, key metrics | Core |
| P1 | Tracing with OpenTelemetry | Distributed tracing for key paths | Core |
| P1 | Documentation complete | All docs reviewed, examples added | All |
| P2 | Model management | Download, update, verify, fallback | Model runtime |
| P2 | Dashboard plugins | Plugin-published dashboard widgets | Plugin host |
| P2 | Source plugin example | Reference implementation for plugin developers | Plugin API |

**Success criteria:**
- [ ] Plugin API is stable and documented
- [ ] At least one third-party plugin developed and tested
- [ ] System handles 2x Phase 2 load without degradation
- [ ] Installation on all platforms in under 5 minutes
- [ ] 99.9% uptime in 30-day test (less than 43 minutes downtime)
- [ ] All metrics and logs accessible via dashboard
- [ ] Documentation passes external review

**Known risks:**
- gRPC overhead may be significant for simple plugins → offer in-process plugin option later
- Performance bottlenecks may require architecture changes → profile before optimization
- Installation complexity varies by platform → invest in CI testing per platform
- Plugin security model must be robust → security review before release

---

### Phase 5: Autonomy

**Goal:** Goal-oriented operation, deep dives, ecosystem growth, peer-to-peer capabilities.

**Duration:** Ongoing (no fixed end date)

**Team:** Variable, community-driven

**Deliverables:**

| Priority | Feature | Description | Dependencies |
|----------|---------|--------------|--------------|
| P0 | Goal-oriented operation | User specifies interests; system proactively deep-dives | Phase 4 |
| P0 | Deep dive research | Given topic/entity, comprehensive analysis | Phase 4 |
| P1 | Entity deep dives | Everything known about an entity | Phase 3 |
| P1 | Event deep dives | Comprehensive event timeline and analysis | Phase 3 |
| P1 | Contradiction deep dives | Detailed contradiction analysis with evidence | Phase 3 |
| P1 | Plugin marketplace | Repository, discovery, one-click install | Phase 4 |
| P1 | Community contribution guide | Clear process for accepting contributions | Phase 4 |
| P2 | Peer-to-peer sync | Share knowledge graphs across trusted instances | Phase 4 |
| P2 | Collaborative narratives | Cross-instance narrative detection | P2P sync |
| P2 | Privacy-preserving federation | Differential privacy for cross-instance stats | P2P sync |
| P3 | Fine-tuning pipeline | Train custom extraction/generation models | Phase 4 |
| P3 | Active learning | User corrections improve extraction quality | Phase 3 |
| P3 | Predictive analytics | Trend prediction, anomaly forecasting | Phase 4 |
| P3 | Multi-language support | Non-English source processing | Phase 4 |

**Success criteria (evolving):**
- [ ] System proactively researches user-specified topics
- [ ] Deep dives produce comprehensive, sourced analyses
- [ ] Plugin marketplace has 10+ community plugins
- [ ] Multiple Objective instances share information securely
- [ ] Community contributions accepted regularly
- [ ] Active learning measurably improves extraction over time

**Known risks:**
- Goal-oriented operation requires significant LLM reasoning → expensive
- P2P sync introduces trust and security challenges → design carefully
- Community growth may be slow → invest in documentation and examples
- Fine-tuning requires ML expertise → may be deferred to community

### Release Cadence

```yaml
releases:
  versioning: "semver"          # major.minor.patch
  
  major:
    frequency: "as needed"      # Breaking changes, new architecture
    criteria: "Phase completion"
    
  minor:
    frequency: "monthly"        # New features, significant improvements
    criteria: "Feature complete, tested"
    
  patch:
    frequency: "as needed"      # Bug fixes, security patches
    criteria: "Fix verified, reviewed"
    
  pre-release:
    alpha: "internal testing only"
    beta: "opt-in testers"
    rc: "public testing before release"
```

### Current Status

**Phase 1:** Planning
**Phase 2:** Not started
**Phase 3:** Not started
**Phase 4:** Not started
**Phase 5:** Not started

## Interfaces

- `repository-structure.md` — code organization per phase
- `coding-standards.md` — quality expectations throughout phases
- `docs/architecture/architecture-decisions.md` — decisions guiding phase priorities

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Phase takes longer than estimated | Delayed subsequent phases | Trim scope to P0/P1; separate P2 for later |
| Key dependency unavailable | Blocked development | Fallback technology identified for each dependency |
| Team capacity insufficient | Slow progress | Prioritize ruthlessly; community contributions for P2+ |
| User feedback contradicts roadmap | Wrong priorities | Quarterly roadmap review with user input |
| Technical debt accumulation | Slows later phases | Budget 20% of each phase for refactoring |
| New technology (Kuzu, LanceDB) rejected by community | Support burden | Use well-known alternatives as fallback |
| ML quality targets not met | System underwhelming | Iterate on prompts, models, and architecture |

## Future Extensions

- Formal user research program to validate roadmap priorities
- Public roadmap with voting (GitHub Discussions)
- Contributor path from user → bug reporter → patch author → core contributor
- Phase 6+ planning deferred until Phase 4 completion
