# Objective

**A local-first intelligence operating system. Runs forever. Listens everywhere. Tells you what matters.**

Objective is not a chatbot. Not a dashboard. Not a summarizer. Objective is a **perpetual intelligence and broadcasting platform** — a long-running daemon that continuously ingests information from across the web, extracts entities and claims, detects events, narratives, and contradictions, maintains a living knowledge graph, and generates written reports and audio broadcasts — all on your own machine.

[![Status](https://img.shields.io/badge/status-pre--alpha-red?style=flat-square)](docs/development/roadmap.md)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey?style=flat-square)](docs/deployment/installation.md)
[![Privacy](https://img.shields.io/badge/privacy-local--first-brightgreen?style=flat-square)](docs/security/privacy-model.md)

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

- **Continuous Ingestion** — RSS, Reddit, YouTube, Podcasts, PDFs, SEC filings, government feeds, blogs, email, Telegram, Discord, GitHub, ArXiv, Hacker News. Add sources once; Objective polls them forever.
- **Knowledge Graph** — Entities, claims, events, narratives, and contradictions stored in a persistent graph database (Kuzu DB) with full temporal provenance and confidence tracking.
- **Intelligence Pipelines** — Entity extraction, claim extraction, relationship extraction, event formation, narrative clustering, contradiction detection — all running locally.
- **Always-On Broadcast** — Generates written reports and audio podcasts on a configurable schedule. Breaking news triggers immediate broadcasts. When nothing is happening, it produces deep dives and system summaries.
- **100% Local-First** — All data, all processing, all inference runs on your machine. Zero cloud dependencies. No data ever leaves without your explicit consent.
- **Extensible** — Plugin system (gRPC) for custom sources, processors, broadcast formats, and notification sinks. Write plugins in any language.
- **Dashboard** — Web UI with real-time feed, event explorer, narrative explorer, interactive knowledge graph, broadcast reader with audio player, and source management.

---

## Quick Start

### Install

Choose your platform:

```bash
# macOS (Homebrew)
brew tap objective/tap && brew install objective

# Linux (APT)
echo "deb https://objective.ai/apt stable main" | sudo tee /etc/apt/sources.list.d/objective.list
sudo apt update && sudo apt install objective

# Windows (Winget)
winget install Objective

# Docker
docker run -d -p 8080:8080 -v objective-data:/var/lib/objective objective/objective:latest
```

### Setup

```bash
objective setup
```

The setup wizard will:
1. Check system requirements
2. Download models (~6 GB: Mistral 7B, Mixtral 8x7B, embedding model, TTS voice)
3. Walk you through adding your first source
4. Configure broadcast schedule

### Run

```bash
objective start --daemon
```

Open the dashboard at **http://localhost:8080**.

---

## Architecture at a Glance

```
┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐
│ INGESTION │─▶│EXTRACTION│─▶│CORRELA-  │─▶│    BROADCAST     │
│  LAYER    │  │  LAYER   │  │ TION     │  │     LAYER        │
│           │  │          │  │ LAYER    │  │                  │
│ Sources   │  │ Entities │  │ Events   │  │ Reports          │
│ Adapters  │  │ Claims   │  │Narratives│  │ Audio            │
│ Scheduler │  │ Relations│  │Contradic.│  │ Streaming        │
└─────┬─────┘  └─────┬────┘  └────┬─────┘  └────────┬─────────┘
      │               │           │                  │
      └───────────────┴───────────┴──────────────────┘
                              │
                      ┌───────┴────────┐
                      │  KNOWLEDGE     │
                      │    GRAPH       │
                      │  (Kuzu DB)     │
                      └────────────────┘
```

**12 internal services** communicate via an event-driven message bus (NATS JetStream), each with well-defined boundaries, inputs, outputs, and failure modes. See [architecture-overview.md](docs/architecture/architecture-overview.md).

---

## Documentation

| Section | Contents |
|---------|----------|
| [Vision](docs/vision/vision.md) | Why Objective exists, philosophy, principles |
| [Architecture](docs/architecture/architecture-overview.md) | System design, 12 ADRs, service boundaries |
| [Data Model](docs/data/knowledge-graph.md) | Graph schema, storage, provenance |
| [Ingestion](docs/ingestion/ingestion-architecture.md) | Source adapters, scheduling, 15 source types |
| [Processing](docs/processing/extraction-engine.md) | Extraction, events, narratives, contradictions |
| [AI / Models](docs/ai/model-strategy.md) | Model selection, prompting, agent architecture |
| [Broadcast](docs/broadcast/broadcast-engine.md) | Infinite operation, scheduling, idle behavior |
| [Audio](docs/broadcast/audio-system.md) | TTS, voice management, podcast generation |
| [API](docs/api/internal-api.md) | REST, WebSocket, plugin API |
| [UI / UX](docs/ui/dashboard-spec.md) | Dashboard spec, UX principles |
| [Deployment](docs/deployment/installation.md) | Installation, operations, observability |
| [Security](docs/security/security-model.md) | Threat model, credential management |
| [Privacy](docs/security/privacy-model.md) | Data ownership, retention, user control |
| [Development](docs/development/repository-structure.md) | Repo layout, coding standards, roadmap |

---

## Roadmap

| Phase | Focus | Status |
|-------|-------|--------|
| **1 — Foundation** | Core pipeline: RSS ingestion, entity/claim extraction, knowledge graph, text broadcast | 📝 Planning |
| **2 — Intelligence** | Events, narratives, contradictions, 15 source types, provenance | 🔜 |
| **3 — Quality** | Audio (TTS/podcast), dashboard UI, breaking news, idle content | 🔜 |
| **4 — Scale** | Plugin system, performance, platform packages, observability | 🔜 |
| **5 — Autonomy** | Goal-oriented research, deep dives, P2P sync, ecosystem | 🔜 |

See the full [roadmap](docs/development/roadmap.md) for details.

---

## Design Decisions

- **Why local-first?** — Privacy, offline operation, zero cloud costs, and full user control. See [ADR-001](docs/architecture/architecture-decisions.md#adr-001-local-first-architecture).
- **Why Kuzu DB?** — Embedded graph database with columnar storage, ACID transactions, and Cypher queries. No server process. See [ADR-002](docs/architecture/architecture-decisions.md#adr-002-kuzu-db-as-knowledge-graph).
- **Why llama.cpp + ONNX?** — Local inference on consumer hardware. llama.cpp for LLMs (Mistral, Mixtral), ONNX for embeddings. See [ADR-004](docs/architecture/architecture-decisions.md#adr-004-local-llm-runtime-llamacpp--onnx).
- **Why Rust?** — Memory safety, concurrency, FFI with C libraries, small binary, cross-platform. See [ADR-009](docs/architecture/architecture-decisions.md#adr-009-rust-as-primary-implementation-language).

All 12 architectural decisions are documented in [architecture-decisions.md](docs/architecture/architecture-decisions.md).

---

## System Requirements

| | Minimum | Recommended |
|--|---------|-------------|
| **CPU** | 4 cores, x86_64 (AVX2) or Apple Silicon | 8 cores |
| **RAM** | 16 GB | 32 GB |
| **Storage** | 20 GB free | 100 GB SSD |
| **GPU** | None (CPU-only mode) | 8 GB VRAM (NVIDIA, AMD, Apple Silicon) |
| **OS** | macOS 13+ / Ubuntu 22.04+ / Windows 10+ | Same |

---

## Technical Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (backend), TypeScript/React (dashboard) |
| Knowledge Graph | [Kuzu DB](https://kuzudb.com/) (embedded, columnar) |
| Vector Store | [LanceDB](https://lancedb.com/) (embedded) |
| Message Bus | NATS JetStream (embedded) |
| LLM Runtime | [llama.cpp](https://github.com/ggerganov/llama.cpp) |
| Embedding Runtime | ONNX Runtime |
| ASR (Audio) | [whisper.cpp](https://github.com/ggerganov/whisper.cpp) |
| TTS | Piper TTS |
| Plugin API | gRPC (protobuf) |
| Dashboard | React + Vite + Tailwind CSS |

---

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) at your option.

---

## Contributing

Objective is in **pre-alpha**. The documentation suite is complete and serves as the foundation for implementation. If you're interested in contributing:

1. Read the [coding standards](docs/development/coding-standards.md)
2. Review the [roadmap](docs/development/roadmap.md) and find an unassigned area
3. Open an issue to discuss your approach
4. Submit a PR

All contributions are subject to the project's [Code of Conduct](CODE_OF_CONDUCT.md).

---

*Built with determination, not venture capital.*
# Objective05 Implementation Notes

Objective05 is now scaffolded as a Rust Cargo workspace following the documented local-first architecture.

## Run Locally

```bash
cargo run -p objective -- setup
cargo run -p objective -- serve
```

The API listens on `http://127.0.0.1:8080` by default.

Implemented endpoints:

- `GET /api/v1/health`
- `GET /api/v1/stats`
- `GET /api/v1/documents`
- `GET /api/v1/extractions`
- `GET /api/v1/events`
- `GET /api/v1/entities`
- `GET /api/v1/entities/summary`
- `GET /api/v1/claims`
- `GET /api-docs/openapi.json`

The OpenAPI document at `/api-docs/openapi.json` aggregates the `utoipa` annotations from every route handler and serves a machine-readable description of the current API surface.

## Development Commands

```bash
make build
make test
make fmt
make clippy
```

## Current MVP Scope

The first vertical slice includes typed core schemas, configuration loading, tracing setup, gzip-backed document archive storage, in-memory extraction storage, in-memory event publication, document normalization, RSS ingestion, heuristic extraction, and API routes for health, stats, documents, extractions, and events.

The in-memory extraction store and bus are temporary MVP implementations. They preserve the documented interfaces so Kuzu, LanceDB, and NATS can replace them without changing service consumers.

Documents are persisted as compressed JSON files under the configured `storage.document_path`, partitioned by fetch year and month.

## Implemented Source Adapters

- `rss`: parses RSS and Atom feeds, preserves feed metadata, supports URL-backed polling, and has offline XML fixture tests.
- `hacker_news`: talks to the public Algolia search API, preserves score / comment / author metadata, and has offline JSON fixture tests.
- `fixture`: an in-memory adapter used by the first vertical slice and HTTP route tests.
