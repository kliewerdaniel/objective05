# Objective

**A local-first intelligence operating system. Runs forever. Listens everywhere. Tells you what matters.**

Objective is not a chatbot. Not a dashboard. Not a summarizer. Objective is a **perpetual intelligence and broadcasting platform** — a long-running daemon that continuously ingests information from across the web, extracts entities and claims, detects events, narratives, and contradictions, maintains a living knowledge graph, and generates written reports and audio broadcasts — all on your own machine.

[![Status](https://img.shields.io/badge/status-alpha-yellow?style=flat-square)](docs/development/roadmap.md)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square)](#license)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey?style=flat-square)](docs/deployment/installation.md)
[![Privacy](https://img.shields.io/badge/privacy-local--first-brightgreen?style=flat-square)](docs/security/privacy-model.md)
[![Rust](https://img.shields.io/badge/rust-1.91-orange?style=flat-square)](rust-toolchain.toml)
[![Tests](https://img.shields.io/badge/tests-316%20%7C%2046-green?style=flat-square)](#tests)

---

## Why Objective

The information landscape has fundamentally changed. Volume, velocity, and complexity now exceed any individual's capacity to track, correlate, and understand.

Existing tools fall short:

| Tool | Problem |
|------|---------|
| **Chatbots** | Ephemeral, stateless, reactive. They answer questions but build no persistent understanding. |
| **Dashboards** | Passive, manual, siloed. They display data but synthesize nothing. |
| **Newsletters** | One-size-fits-all, at someone else's schedule, with someone else's bias. |

Objective bridges this gap with a **third category**: perpetual intelligence. It continuously ingests, correlates, and communicates — building a persistent knowledge graph that deepens over time, generating broadcasts on your schedule, and keeping you informed even when you're not watching.

---

## Features

- **Continuous Ingestion** — 10 source adapters covering RSS, Reddit, YouTube, podcasts, SEC EDGAR filings, GitHub (events + releases), ArXiv, Hacker News, and the open web. Add a source once; Objective polls it forever. The full set of source types is enumerated in `SourceType` in `crates/ingestion/src/registry.rs`.
- **Knowledge Graph** — Entities, claims, events, narratives, and contradictions stored in a persistent graph database with full temporal provenance and confidence tracking. Real Kuzu DB backend is opt-in via the `kuzu` Cargo feature; an in-memory stub ships by default.
- **Intelligence Pipelines** — Heuristic entity, claim, and relationship extraction running today. Local LLM + ONNX embedding extraction layered on top via the `llama` and `onnx` Cargo features.
- **Event, Narrative, and Contradiction Engines** — Correlate claims into derived events, group events into narratives, and surface contradictory claims with severity scoring.
- **Always-On Broadcast** — Generates written reports on a configurable schedule. `POST /api/v1/broadcasts/generate` produces on-demand drafts. Audio podcasts and TTS pipeline are on the roadmap.
- **100% Local-First** — All data, all processing, all inference runs on your machine. Zero cloud dependencies. No data ever leaves without your explicit consent.
- **Extensible** — In-process plugin host with built-in `audit-log` and `re-emitter` plugins. The plugin API is designed for an eventual gRPC upgrade.
- **Dashboard** — Vite + React 19 + TypeScript SPA with real-time feed, event explorer, narrative explorer, interactive knowledge graph, broadcast reader, and source manager. 46 unit + component tests, no daemon required.
- **Operable** — First-class `GET /api/v1/monitoring` and `GET /api/v1/recovery` endpoints, a `RecoveryService` watchdog that emits `system.service.crash` and `system.service.recovered` events, and a `WebSocketHub` bridging the message bus to `/ws`.

---

## Quick Start

### Prerequisites

- **Rust 1.91.0** (pinned via `rust-toolchain.toml`)
- **Node 20+** and **npm** (for the dashboard)
- **macOS 13+** or **Linux** (Ubuntu 22.04+ recommended)
- **CMake, Clang, libclang** (only if you build with `--features kuzu`)

### Run the daemon

```bash
git clone https://github.com/anomalyco/objective
cd objective
cargo run -p objective -- setup   # initialize .objective/ in the current directory
cargo run -p objective -- serve   # API on http://127.0.0.1:8080
```

The API listens on `http://127.0.0.1:8080`. Verify it with:

```bash
curl http://127.0.0.1:8080/api/v1/health
```

### Run the dashboard

In a second terminal:

```bash
cd dashboard
npm install
npm run dev    # http://localhost:5173, proxies /api and /ws to the daemon
```

### Common commands

```bash
make build     # cargo build --workspace
make test      # cargo test --workspace
make fmt       # cargo fmt --all
make clippy    # cargo clippy --workspace --all-targets -- -D warnings
make run       # cargo run -p objective -- serve
```

### Optional features

| Feature | What it adds | Build cost |
|---------|--------------|------------|
| `kuzu` | Real Kuzu DB-backed `KuzuGraphStore` (replaces the in-memory stub) | ~1m40s first compile on Apple Silicon (C++ build chain) |
| `onnx` | Real `ort::Session` path in `OnnxRuntime` for embedding inference | Downloads the ONNX runtime binary at build time |
| `llama` | Real `llama_cpp` session in `LlamaRuntime` for NER, claim, and relation extraction | Vendors llama.cpp via `llama_cpp_sys` |

```bash
# Real knowledge graph + LLM + ONNX embedding
cargo test --workspace --features kuzu,llama,onnx

# Real knowledge graph only
cargo test -p objective-store --features kuzu

# Parallel C++ build for the Kuzu feature
CMAKE_BUILD_PARALLEL_LEVEL=$(sysctl -n hw.ncpu) cargo test -p objective-store --features kuzu
```

Default builds remain free of native dependencies and run the deterministic stubs in place of the real model runtimes.

---

## What's Working Today

The implementation follows the architecture documented under `docs/`, with progress tracked in [`TASKS.md`](TASKS.md). As of the current snapshot, all foundation, data layer, core services, APIs, background systems, and dashboard work is complete and tested.

### Workspace crates (11)

| Crate | Role |
|-------|------|
| `objective-core` | Typed schemas, `GraphRepository` / `VectorRepository` / `ModelRuntime` traits, config, errors |
| `objective-message-bus` | NATS JetStream wrapper with in-memory fallback for tests |
| `objective-store` | Document archive, Kuzu graph store (real + stub), LanceDB vector store (stub), event repo, snapshot/backup |
| `objective-ingestion` | Document normalizer, ingestion service, 15 source adapters, scheduler-driven polling |
| `objective-extraction` | Heuristic + model-runtime-backed entity/claim/relation extraction, prompts |
| `objective-correlation` | Event, narrative, and contradiction engines |
| `objective-model-runtime` | `NoopRuntime`, `OnnxRuntime`, `LlamaRuntime`, `LocalModelRuntime` composite, slot state machines, queues, histograms |
| `objective-scheduler` | Cron parser, job definitions, state persistence, event emission |
| `objective-plugin-host` | Plugin lifecycle, manifest discovery, bus event routing, built-in `audit-log` + `re-emitter` |
| `objective-api-gateway` | Axum-based REST + WebSocket gateway, OpenAPI annotations, recovery service, monitoring publisher |
| `objective` | The binary that wires it all together (`setup`, `serve`, etc.) |

### Source adapters (10 + 1 fixture)

`rss`, `hacker_news`, `reddit`, `youtube`, `podcast`, `sec_edgar`, `github` (events), `github_releases`, `arxiv`, `web`, plus the `static` fixture used by integration tests. The set is enumerated by `SourceType` in `crates/ingestion/src/registry.rs`; adapter construction is dispatched by `SourceType::as_str()` matching the `source_type` field on `SourceDefinition`. PDF, government-feed, blog, email, Telegram, and Discord adapters are documented in the architecture but not yet implemented; see the [roadmap](docs/development/roadmap.md).

### REST + WebSocket API (40 paths, 45 method combinations)

All paths are mounted in `crates/api-gateway/src/server.rs`:

- **Read** (33 `GET` endpoints) — `/api/v1/{health, stats, documents, extractions, events, entities, entities/summary, entities/:name, claims, sources, source-registry, source-registry/:name, narratives, narratives/:id, contradictions, contradictions/:id, broadcasts, broadcasts/latest, broadcasts/:id, derived-events, derived-events/top, derived-events/:id, monitoring, recovery, export, search, config, plugins, plugins/:name, model-runtime}` · `/api-docs/openapi.json` · `/ws` (WebSocket upgrade)
- **Write** (10 `POST` endpoints) — `/api/v1/{entities/merge, events/:id/resolve, broadcasts/generate, recovery/check, source-registry, source-registry/:name/trigger, contradictions/:id/resolve, plugins/:name/restart, plugins/reload, model-runtime/reload}`
- **Mutate** (1 `PUT`, 1 `DELETE`) — `/api/v1/source-registry/:name`

The full machine-readable schema is served at `/api-docs/openapi.json` and is generated from `utoipa` annotations on every route handler.

### Tests

- **316 backend unit + integration tests** with `cargo test --workspace --features kuzu`
- **307 backend tests** with the default `cargo test --workspace` (in-memory stub path)
- **46 dashboard unit + component tests** with `npm test` in `dashboard/`
- **Clippy** is clean on the entire workspace; the `Makefile` runs it with `-D warnings`

---

## Architecture at a Glance

```
┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐
│ INGESTION │─▶│EXTRACTION│─▶│CORRELA-  │─▶│    BROADCAST     │
│  LAYER    │  │  LAYER   │  │ TION     │  │     LAYER        │
│           │  │          │  │ LAYER    │  │                  │
│ Sources   │  │ Entities │  │ Events   │  │ Reports          │
│ Adapters  │  │ Claims   │  │Narratives│  │ Audio (planned)  │
│ Scheduler │  │ Relations│  │Contradic.│  │ Streaming        │
└─────┬─────┘  └─────┬────┘  └────┬─────┘  └────────┬─────────┘
      │               │           │                  │
      └───────────────┴───────────┴──────────────────┘
                               │
                       ┌───────┴────────┐
                       │  KNOWLEDGE     │
                       │    GRAPH       │
                       │ (Kuzu / stub)  │
                       └────────────────┘
```

Eleven internal services communicate via an event-driven message bus (NATS JetStream), each with well-defined boundaries, inputs, outputs, and failure modes. The model runtime is layered behind a single `ModelRuntime` trait so the deterministic stub stays the default while real `onnx` + `llama.cpp` paths can be opted in.

See [`docs/architecture/architecture-overview.md`](docs/architecture/architecture-overview.md) for the full design and [`docs/architecture/architecture-decisions.md`](docs/architecture/architecture-decisions.md) for the 12 ADRs.

---

## Documentation

| Section | Contents |
|---------|----------|
| [Vision](docs/vision/vision.md) | Why Objective exists, philosophy, principles |
| [Architecture](docs/architecture/architecture-overview.md) | System design, 12 ADRs, service boundaries |
| [Data Model](docs/data/knowledge-graph.md) | Graph schema, storage, provenance, current Kuzu implementation |
| [Storage](docs/data/storage-architecture.md) | Disk layout, document archive, snapshots, backup |
| [Ingestion](docs/ingestion/ingestion-architecture.md) | Source adapters, scheduling, source-type dispatch |
| [Processing](docs/processing/extraction-engine.md) | Heuristic + model extraction, events, narratives, contradictions |
| [Model Runtime](docs/processing/model-runtime.md) | `ModelRuntime` trait, OnnxRuntime, LlamaRuntime, slot state machine, queues |
| [Broadcast](docs/broadcast/broadcast-engine.md) | Infinite operation, scheduling, idle behavior |
| [API](docs/api/internal-api.md) | REST, WebSocket, plugin API |
| [UI / UX](docs/ui/dashboard-spec.md) | Dashboard spec, UX principles |
| [Deployment](docs/deployment/installation.md) | Long-term packaging, operations, observability |
| [Deployment (from source)](docs/deployment/from-source.md) | Prerequisites, local dev, Docker build/run, candidate bundles |
| [Security](docs/security/security-model.md) | Threat model, credential management |
| [Privacy](docs/security/privacy-model.md) | Data ownership, retention, user control |
| [Development](docs/development/repository-structure.md) | Repo layout, coding standards, roadmap |

---

## Design Decisions

- **Why local-first?** — Privacy, offline operation, zero cloud costs, and full user control. See [ADR-001](docs/architecture/architecture-decisions.md#adr-001-local-first-architecture).
- **Why Kuzu DB?** — Embedded graph database with columnar storage, ACID transactions, and Cypher queries. No server process. See [ADR-002](docs/architecture/architecture-decisions.md#adr-002-kuzu-db-as-knowledge-graph). (Kuzu is feature-gated in the build; default builds use an in-memory stub that preserves the same `GraphRepository` trait.)
- **Why llama.cpp + ONNX?** — Local inference on consumer hardware. llama.cpp for LLMs, ONNX for embeddings. Both ship as opt-in Cargo features so default builds stay free of native deps. See [ADR-004](docs/architecture/architecture-decisions.md#adr-004-local-llm-runtime-llamacpp--onnx).
- **Why Rust?** — Memory safety, concurrency, FFI with C libraries, small binary, cross-platform. See [ADR-009](docs/architecture/architecture-decisions.md#adr-009-rust-as-primary-implementation-language).

All 12 architectural decisions are documented in [architecture-decisions.md](docs/architecture/architecture-decisions.md).

---

## System Requirements

| | Minimum | Recommended |
|--|---------|-------------|
| **CPU** | 4 cores, x86_64 (AVX2) or Apple Silicon | 8 cores |
| **RAM** | 8 GB (heuristic-only mode) / 16 GB (with `llama` + `onnx`) | 32 GB |
| **Storage** | 5 GB (source code + build artifacts) + data | 50 GB SSD for long-running deployments |
| **GPU** | None (CPU-only) | 8 GB VRAM (NVIDIA, AMD, Apple Silicon Metal) for `llama` + `onnx` |
| **OS** | macOS 13+ / Ubuntu 22.04+ | Same |

For the `--features kuzu` build, you'll also need `cmake`, `clang`, and `libclang` available on `$PATH`.

---

## Technical Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust 1.91 (backend), TypeScript + React 19 (dashboard) |
| Knowledge Graph | [Kuzu DB](https://kuzudb.com/) (embedded, columnar) — `kuzu` feature |
| Vector Store | [LanceDB](https://lancedb.com/) (embedded) — stub today, real backend pending |
| Message Bus | NATS JetStream (embedded) |
| LLM Runtime | [llama.cpp](https://github.com/ggerganov/llama.cpp) via `llama-cpp-rs` — `llama` feature |
| Embedding Runtime | [ONNX Runtime](https://onnxruntime.ai/) via `ort` — `onnx` feature |
| ASR (planned) | [whisper.cpp](https://github.com/ggerganov/whisper.cpp) |
| TTS (planned) | Piper TTS |
| Plugin API | In-process trait today; gRPC (protobuf) on the roadmap |
| Dashboard | React 19 + Vite + Tailwind CSS |

---

## License

Dual-licensed under [MIT](https://opensource.org/licenses/MIT) or [Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0) at your option. The workspace `Cargo.toml` carries the `MIT` license string; the full MIT and Apache 2.0 license texts are vendored at the root of the published crate on crates.io.

---

## Contributing

Objective is in **alpha**. The first vertical slice is shipped, the trait boundaries are stable, and the next major push is on real LanceDB, audio (TTS/whisper.cpp), and the gRPC plugin host. If you're interested in contributing:

1. Read the [coding standards](docs/development/coding-standards.md).
2. Skim [TASKS.md](TASKS.md) to see what's done and what's next.
3. Skim the [roadmap](docs/development/roadmap.md) for long-horizon direction.
4. Open an issue to discuss your approach before sending a PR.

---

*Built with determination, not venture capital.*
