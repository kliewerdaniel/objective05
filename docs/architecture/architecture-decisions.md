# Architecture Decisions

## Purpose

Document all significant architectural decisions made during the design of Objective, including alternatives considered, tradeoffs evaluated, and the rationale for each decision.

## Scope

This document covers decisions about technology choices, system architecture, data flow, processing models, deployment strategies, and integration patterns. It is a living document updated as decisions are made.

## Responsibilities

- Preserve the rationale behind architectural decisions
- Prevent re-litigation of settled decisions
- Provide context for new contributors
- Document known tradeoffs and their implications
- Serve as a reference for future architectural reviews

## Assumptions

- Documentation is more valuable than the precise wording of decisions
- New information may invalidate past decisions (decisions are not binding)
- ADR format provides sufficient structure for readability
- Not every decision needs an ADR; only architecturally significant ones

## Design

### ADR Format

Each decision follows this template:

```
# ADR-NNN: Title

## Status
[Proposed | Accepted | Deprecated | Superseded]

## Context
What is the issue motivating this decision?

## Decision
What is the change being made?

## Consequences
What becomes easier or more difficult?

## Compliance
How is this decision enforced?
```

---

### ADR-001: Local-First Architecture

**Status:** Accepted

**Context:** The product philosophy requires that all data and processing remain on the user's machine. No cloud dependency is acceptable for core functionality. However, many modern AI systems depend on cloud APIs for inference, vector search, and storage.

**Decision:** All core services run as local processes. Inference uses locally-hosted models via llama.cpp or ONNX runtime. Vector search uses a local embedded database (LanceDB). The message bus is an embedded NATS server. The knowledge graph is Kuzu DB (embedded graph database). No external network calls are required for processing.

**Consequences:**
- Easier: Privacy guarantees, offline operation, low latency, no cloud costs
- Harder: Model availability (local models are less capable than GPT-4 class), compute requirements, storage management, updates
- Tradeoff: Capability vs. privacy; mitigated by supporting model upgrades as local models improve

**Compliance:** All build pipelines must pass with network disabled. Integration tests run entirely offline.

---

### ADR-002: Kuzu DB as Knowledge Graph

**Status:** Accepted (with feature-gated backend)

**Context:** The system requires a graph database to store entities, claims, events, narratives, and contradictions with temporal provenance. Options considered: Kuzu DB (embedded, columnar), Neo4j (server, heavyweight), Dgraph (distributed, complex), SQLite with graph extensions (limited query capability), custom RDF store (maintenance burden).

**Decision:** Use Kuzu DB as the primary knowledge graph store. Kuzu is embedded (no separate server process), columnar (fast analytical queries), supports Cypher queries, has ACID transactions, and is designed for local-first applications.

**Implementation note:** The Rust binding is wired up behind the `kuzu` Cargo feature on `crates/store`. The default build ships an in-memory stub that implements the same `GraphRepository` trait (the type alias `objective_store::kuzu::KuzuGraphStore` resolves to the stub by default and to the Kuzu-backed impl when the feature is enabled). This keeps first-compile and CI fast while still giving operators a real, durable backend in production builds (`cargo build --release --features kuzu`). See `docs/data/knowledge-graph.md` for the current schema and `crates/store/src/kuzu/real.rs` for the implementation.

**Consequences:**
- Easier: Embedding, deployment, backup (just copy files), analytical queries over large graphs
- Harder: Limited community size, no built-in replication, Cypher subset (not full Cypher), Kuzu project was archived upstream (last published crate `kuzu = 0.11.3`, Oct 2025; community successor is LadybugDB); cxx version must be pinned exactly (1.0.138) so cxx-build and cxx agree on the cxxbridge symbol suffix
- Tradeoff: Embedded simplicity vs. server-based features; acceptable for single-machine deployment

**Compliance:** All graph operations use Kuzu's C API through the language binding.

---

### ADR-003: Durable Message Bus (NATS)

**Status:** Accepted

**Context:** Services need asynchronous, reliable communication. Options: NATS (embedded, JetStream for durability), RabbitMQ (battle-tested, heavy), Redis Queue (simple, no durability guarantees), ZeroMQ (no broker, complex), custom in-process queues (not durable across restarts).

**Decision:** Use NATS in embedded mode with JetStream enabled for durable queues. NATS provides pub/sub, request/reply, and queue groups with exactly-once delivery semantics. JetStream provides persistence across restarts.

**Consequences:**
- Easier: Embedded deployment, durable queues, exactly-once delivery, stream replay for recovery
- Harder: Configuration complexity, JetStream learning curve, storage for stream data
- Tradeoff: Operational simplicity of embedded vs. feature-rich standalone brokers; acceptable for initial deployment

**Compliance:** All inter-service communication uses NATS. No service-to-service direct HTTP calls.

---

### ADR-004: Local LLM Runtime (llama.cpp + ONNX)

**Status:** Accepted

**Context:** The system requires local inference for extraction, embedding, and generation. Options: llama.cpp (best for LLM inference on consumer hardware), ONNX Runtime (best for embedding models and small models), transformers (Python, memory-heavy), vLLM (GPU-focused, heavy), ollama (wraps llama.cpp, less control).

**Decision:** Use llama.cpp for LLM inference (entity extraction, claim extraction, narrative formation, report generation) and ONNX Runtime for embedding models (text embeddings, classification). Both support CPU and GPU inference, are embeddable as libraries, and have language bindings for Rust/Go.

**Consequences:**
- Easier: Local inference, broad hardware support, embeddable as shared libraries
- Harder: Two runtimes to maintain, model download management, prompt format differences across runtimes
- Tradeoff: Two runtimes is more complex than one, but each is optimal for its task class

**Compliance:** All model interactions go through the Model Runtime service, which abstracts the underlying inference engine.

---

### ADR-005: Embedded Vector Store (LanceDB)

**Status:** Accepted

**Context:** The system needs vector storage for semantic search, document similarity, and embedding caching. Options: LanceDB (embedded, columnar, fast), ChromaDB (embedded, simpler, less performant), Qdrant (server-based), Milvus (heavy, distributed), pgvector (requires PostgreSQL).

**Decision:** Use LanceDB as the embedded vector store. LanceDB stores vectors in Lance columnar format, supports hybrid search (vector + metadata filtering), is embeddable as a library, and handles large-scale vector storage efficiently.

**Consequences:**
- Easier: Embedding, no external server, fast similarity search, ACID transactions
- Harder: Smaller community than Qdrant, limited advanced features (no quantization yet)
- Tradeoff: Embedded simplicity vs. server-based features; appropriate for single-machine deployment

**Compliance:** Vector queries use LanceDB's API. Schema is defined in the storage-architecture document.

---

### ADR-006: Event-Driven Architecture with Durable Queues

**Status:** Accepted

**Context:** The processing pipeline has multiple stages that should operate concurrently. A synchronous pipeline would: (a) couple stages together, (b) make recovery on failure harder, (c) prevent parallel processing, (d) make it impossible to add new processing stages without modifying existing ones.

**Decision:** Use event-driven architecture where each processing stage subscribes to events and emits new events. Events are persisted in NATS JetStream streams. Each consumer tracks its own cursor. Failed events are retried with backoff before moving to a dead-letter stream.

**Consequences:**
- Easier: Loose coupling, independent scaling, replay for recovery, extensibility
- Harder: Eventual consistency, debugging across stages, tracing complexity
- Tradeoff: Consistency model; mitigated by idempotent consumers and deterministic processing

**Compliance:** No processing stage directly calls another stage. All communication is through events.

---

### ADR-007: Timestamp-Priority Merge for Knowledge Graph Updates

**Status:** Accepted

**Context:** Multiple sources may report the same entity or claim with different confidence levels and timestamps. The system needs a deterministic strategy for merging conflicting information.

**Decision:** Use timestamp-priority merge with source confidence as a tiebreaker. When two graph objects conflict:
1. The object with the more recent timestamp wins
2. If timestamps are equal (within tolerance), the object from the higher-confidence source wins
3. Both versions are retained as historical revisions
4. Confidence decays over time without re-verification

**Consequences:**
- Easier: Deterministic merge, temporal reasoning, audit trail
- Harder: Storage grows with revisions, merge complexity
- Tradeoff: Storage cost vs. information preservation; mitigated by configurable retention policies

**Compliance:** All graph write operations use the merge function defined in the provenance model.

---

### ADR-008: Periodic + Event-Triggered Processing

**Status:** Accepted

**Context:** Some processing should happen on a schedule (e.g., polling RSS feeds every hour) while other processing should happen immediately (e.g., extraction when a document arrives). The system needs a unified triggering mechanism.

**Decision:** Use a Scheduler service for periodic jobs (ingestion polls, periodic broadcast generation, maintenance tasks). Event-triggered processing is handled by the message bus (extraction subscribes to document events, correlation subscribes to extraction events). The Scheduler emits events that trigger pipeline runs, making it consistent with the event-driven model.

**Consequences:**
- Easier: Unified triggering model, all processing is event-driven
- Harder: Scheduler needs to manage cron-like state across restarts
- Tradeoff: Simplicity of unified model vs. specialization for periodic vs. immediate processing

**Compliance:** No hard-coded timers or sleep loops in processing services. All timing goes through the Scheduler.

---

### ADR-009: Rust as Primary Implementation Language

**Status:** Accepted

**Context:** The system requires a language with: (a) low resource usage for daemon operation, (b) strong safety guarantees for long-running processes, (c) excellent concurrency support, (d) good FFI for embedding C/C++ libraries (llama.cpp, Kuzu), (e) cross-platform support.

**Decision:** Use Rust as the primary implementation language. Go was the primary alternative, offering simpler concurrency but worse FFI and higher memory usage. Python was rejected due to memory overhead and concurrency limitations.

**Consequences:**
- Easier: Memory safety, concurrency, FFI with C libraries, small binary size, cross-compilation
- Harder: Compile times, ownership model learning curve, ecosystem maturity in niche areas
- Tradeoff: Development speed vs. runtime safety and performance; acceptable for a long-running system

**Compliance:** Primary services are in Rust. Plugins may be in any language via the plugin API (gRPC).

---

### ADR-010: Plugin Architecture via gRPC

**Status:** Accepted

**Context:** The system needs extension points for source connectors, custom processors, and broadcast formats. These should not require recompilation of the core system.

**Decision:** Use gRPC for the plugin API. Plugins are separate processes that communicate with the core via defined protobuf services. This allows plugins in any language and provides strong contract enforcement via protobuf schemas.

**Consequences:**
- Easier: Language-agnostic plugins, strong typing via protobuf, streaming support
- Harder: Process management for plugins, gRPC overhead for simple plugins
- Tradeoff: Process isolation overhead vs. flexibility; acceptable for plugin workloads

**Compliance:** All plugins implement the protobuf-defined gRPC service. Plugin discovery uses a well-known directory.

---

### ADR-011: Temporal Knowledge Graph Model

**Status:** Accepted

**Context:** The knowledge graph must track not just current state but the evolution of entities, claims, and relationships over time. A snapshot-only model would lose historical context needed for narrative detection and contradiction analysis.

**Decision:** Every node and edge in the knowledge graph carries temporal metadata: `valid_from`, `valid_to`, `created_at`, `updated_at`, and `superseded_by`. The graph supports time-range queries (e.g., "what did we know about X between dates A and B"). Deletion is logical (mark with `valid_to`) rather than physical.

**Consequences:**
- Easier: Temporal queries, contradiction detection, narrative evolution tracking, audit
- Harder: Query complexity, storage growth, index overhead
- Tradeoff: Storage cost vs. temporal reasoning capability; mitigated by configurable archiving

**Compliance:** All graph schemas include temporal fields. All queries that need current state filter on `valid_to IS NULL`.

---

### ADR-012: Single Binary Distribution

**Status:** Accepted

**Context:** Installation must be trivially simple. A multi-service installation with separate binaries, configuration files, and dependency management would violate the "minutes to install" goal.

**Decision:** Distribute Objective as a single binary that embeds NATS, spawns all services as async tasks within the same process, and manages lifecycle internally. This is NOT a monolith in the coupling sense — services communicate via NATS as if they were separate processes, but they share an address space for deployment simplicity.

**Consequences:**
- Easier: Installation (single binary), deployment (single process), updates (replace binary)
- Harder: Failure isolation (one process crash kills everything), resource sharing (one OOM kills everything)
- Tradeoff: Fail-safe mitigated by: (a) internal supervision tree, (b) resource limits per service, (c) health checks that restart individual services within the process

**Compliance:** Build pipeline produces a single binary. Integration tests run against the single-binary mode.

---

### ADR-013: Plugin Host v1 Is In-Process

**Status:** Accepted (subject to ADR-015 below)

**Context:** `docs/api/plugin-api.md` describes a gRPC-based plugin contract with external plugin processes, but shipping the full gRPC host (proto, server, schema migration, packaging, IPC, crash containment across process boundaries) is a substantial engineering effort that competes with the documented MVP scope (RSS ingestion, heuristic extraction, knowledge graph, broadcast drafts).

**Decision:** Ship the v1 plugin host (`crates/plugin-host`, `objective_plugin_host`) fully in-process. Built-in plugins are `Arc<dyn Plugin>` objects registered at startup via `PluginHost::register_builtin`. Discovered manifests under `.objective/plugins/<name>/plugin.json` are mounted as `NoopPlugin` placeholders so they show up in `/api/v1/plugins` and the lifecycle counts. The gRPC contract from `docs/api/plugin-api.md` is the future upgrade path; the API surface and lifecycle states mirror the spec so the swap is mechanical.

**Consequences:**
- Easier: V1 ships today with two useful built-ins (`audit-log` recording every event the host routes to it, and `re-emitter` republishing `extraction.document.processed` events on `plugin.re_emitted`); the API, manifests, lifecycle states, persistent state file, and restart counts are all already on the contract
- Harder: External plugin authors cannot ship executables yet; the only way to extend is to add a Rust crate to the workspace
- Tradeoff: V1 keeps the user-facing surface stable so the upgrade to out-of-process plugins does not require an API migration; only the host implementation changes

**Compliance:** `PluginHost::register_builtin` is the only registration path; `discover_into` is the only discovery path; `Plugin::handle` returns `PluginOutput::{Log,Publish,State}` which is the superset the future gRPC contract will marshal. The v1 host never spawns child processes and never opens a gRPC socket.

---

### ADR-014: Source Registry Persists to a Single JSON File

**Status:** Accepted

**Context:** The documented MVP exposes a "manage your ingestion sources at runtime" surface so operators can add, update, and remove adapters without restarting the daemon. The store needs to survive restarts and be readable from both the API gateway and the pipeline worker without a separate database engine.

**Decision:** Persist the `SourceRegistry` as a single JSON file at `.objective/state/sources.json`. The store is a `tokio::sync::RwLock<HashMap<String, SourceDefinition>>` with a per-mutation `persist()` call. A new file is written before any in-memory state is updated, but the current implementation does not perform a write-temp-then-rename — the simplicity tradeoff is documented in the source. The store seeds `hackernews_front` (hnrss frontpage) and `lobsters` on first boot.

**Consequences:**
- Easier: Zero external dependencies, trivial to inspect (`cat .objective/state/sources.json`), trivial to back up, schema-validated by the same `SourceDefinition` type the API uses
- Harder: Concurrent write throughput is bounded by the JSON serialisation cost; for the documented MVP throughput (handful of CRUD operations) this is fine
- Tradeoff: A future "lots of sources" or "multi-writer" scenario would migrate to the graph store; the trait boundary on `SourceRegistry` keeps that swap mechanical

**Compliance:** No code path mutates the source set without going through `SourceRegistry::{add,update,remove,trigger}`. The pipeline worker only ever reads through `SourceRegistry::list` / `SourceRegistry::spawn_adapter`.

---

### ADR-015: Model Runtime Integration Is a Trait-First, Phased Delivery

**Status:** Accepted

**Context:** ADR-004 commits to local llama.cpp + ONNX as the inference substrate, and `docs/ai/model-strategy.md` lays out the task-model mapping, the lifecycle, and the resource budget. The extraction crate today ships a single implementation: `HeuristicExtractionService`, a regex-based stub behind the `DocumentProcessor` trait. The MVP ships green without a real model, but the documented roadmap promises a runtime-backed extractor. The question is the shape of the v1 cut: do we land llama.cpp + ONNX now, or do we land the trait surface and phase the native dependencies in behind feature flags?

**Decision:** Land the integration in four phases, each shippable behind a `local-models` Cargo feature so the default build stays small and the C++ toolchain is not required to compile or test the daemon. The full design is in `docs/processing/model-runtime.md`. Summary:

- **Phase 1 (this design)**: define `ModelRuntime` / `InferenceTask` / `InferenceResult` / `ModelError` in `objective-core::traits`; ship `NoopRuntime` in a new `crates/model-runtime`; land `RuntimeExtractionService` in `crates/extraction` with the orchestrator code and a heuristic fallback; add `ModelRuntimeConfig::{Disabled, Heuristic, Local}` to `ObjectiveConfig`; default is `Disabled` (v0 behaviour preserved).
- **Phase 2**: add the `ort` (ONNX Runtime) dependency behind the `onnx` feature and implement `InferenceKind::Embedding`.
- **Phase 3**: add the `llama-cpp-rs` dependency behind the `llama` feature and implement `NER` / `ClaimExtraction` / `RelationExtraction`.
- **Phase 4**: lifecycle, observability, and per-model inference queues.

**Consequences:**
- Easier: Phase 1 lands the architecture (trait surface, config plumbing, fallback semantics, tests) without the C++ toolchain, so the design can be reviewed and the upgrade to llama.cpp/ONNX is mechanical; the gRPC inference service from `docs/ai/model-strategy.md` is the long-term shape of the same trait, so the upgrade path there is also mechanical
- Harder: Reviewers have to trust the design rather than see a working end-to-end inference call in the first PR
- Tradeoff: Same precedent as ADR-013 (in-process plugin host): the long-term gRPC inference service is the upgrade path; the in-process trait is the v1

**Compliance:** Every call site that wants to use a model goes through `Arc<dyn ModelRuntime>`. No call site imports `llama-cpp-rs` or `ort` directly outside of `crates/model-runtime/src/local.rs`. The `DocumentProcessor` trait remains the only thing the pipeline worker depends on; the runtime choice is an `AppState` decision, not a pipeline decision.

---

## Interfaces

- `architecture-overview.md` — system architecture context
- `service-boundaries.md` — service definitions that decisions enable
- `data/storage-architecture.md` — storage decisions effect

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| ADR not followed | Architecture drift | Code review checklist includes ADR compliance |
| Wrong decision | Suboptimal system | ADRs can be superseded; revisit decisions quarterly |
| Unwritten ADR | Lost rationale | Mandatory ADR for any architectural change |
| Over-documented trivial decisions | Noise | ADR required only for architecturally significant decisions |

## Future Extensions

- ADR-016: Multi-machine distribution strategy (Phase 3)
- ADR-017: Peer-to-peer knowledge graph sync protocol
- ADR-018: Plugin marketplace security model
