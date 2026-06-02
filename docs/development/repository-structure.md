# Repository Structure

## Purpose

Define the ideal repository layout for Objective, organizing code, documentation, tests, and configuration for clarity, maintainability, and developer productivity.

## Scope

This document covers the monorepo layout, service directories, library organization, documentation structure, test organization, and build configuration.

## Responsibilities

- Define the canonical repository structure
- Specify directory naming conventions
- Define separation between services and libraries
- Document test organization
- Specify build and CI configuration layout

## Assumptions

- Monorepo with all services in a single repository
- Rust as primary language (see ADR-009)
- Cargo workspace for dependency management
- gRPC protobuf definitions shared across services
- Dashboard is a separate frontend project within the monorepo

## Design

### Repository Layout

```
objective/
├── Cargo.toml                          # Workspace root
├── Cargo.lock
├── rust-toolchain.toml                 # Rust toolchain specification
├── Makefile                            # Top-level build commands
├── README.md                           # Project overview
│
├── docs/                               # Documentation (as defined in this spec)
│   ├── vision/
│   ├── architecture/
│   ├── data/
│   ├── ingestion/
│   ├── processing/
│   ├── ai/
│   ├── broadcast/
│   ├── api/
│   ├── ui/
│   ├── deployment/
│   ├── security/
│   └── development/
│
├── proto/                              # Protobuf definitions
│   ├── objective/
│   │   ├── common/
│   │   │   └── v1/
│   │   │       ├── types.proto         # Common types
│   │   │       └── events.proto        # Event envelope
│   │   ├── ingestion/
│   │   │   └── v1/
│   │   │       └── ingestion.proto
│   │   ├── extraction/
│   │   │   └── v1/
│   │   │       └── extraction.proto
│   │   ├── correlation/
│   │   │   └── v1/
│   │   │       ├── correlation.proto
│   │   │       ├── event.proto
│   │   │       ├── narrative.proto
│   │   │       └── contradiction.proto
│   │   ├── broadcast/
│   │   │   └── v1/
│   │   │       └── broadcast.proto
│   │   ├── model/
│   │   │   └── v1/
│   │   │       └── model_runtime.proto
│   │   ├── graph/
│   │   │   └── v1/
│   │   │       └── knowledge_graph.proto
│   │   ├── plugin/
│   │   │   └── v1/
│   │   │       └── plugin.proto
│   │   └── system/
│   │       └── v1/
│   │           ├── health.proto
│   │           └── scheduler.proto
│   └── README.md                       # Protobuf conventions
│
├── crates/                             # Rust libraries & services
│   ├── core/                           # Core types, traits, utilities
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types/                  # Core data types
│   │       │   ├── mod.rs
│   │       │   ├── document.rs
│   │       │   ├── entity.rs
│   │       │   ├── claim.rs
│   │       │   ├── event.rs
│   │       │   ├── narrative.rs
│   │       │   ├── contradiction.rs
│   │       │   └── source.rs
│   │       ├── traits/                 # Core traits (SourceAdapter, etc.)
│   │       │   ├── mod.rs
│   │       │   ├── source_adapter.rs
│   │       │   ├── processor.rs
│   │       │   └── storage.rs
│   │       ├── error.rs                # Error types
│   │       ├── config.rs               # Configuration
│   │       └── telemetry/             # Metrics, logging, tracing
│   │           ├── mod.rs
│   │           ├── metrics.rs
│   │           └── tracing.rs
│   │
│   ├── ingestion/                      # Ingestion Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs              # Main service loop
│   │       ├── scheduler.rs            # Poll scheduler
│   │       ├── adapters/               # Source adapters
│   │       │   ├── mod.rs
│   │       │   ├── rss.rs
│   │       │   ├── reddit.rs
│   │       │   ├── youtube.rs
│   │       │   ├── podcast.rs
│   │       │   ├── pdf.rs
│   │       │   ├── blog.rs
│   │       │   ├── sec_edgar.rs
│   │       │   ├── govt_feed.rs
│   │       │   ├── web_page.rs
│   │       │   ├── email.rs
│   │       │   ├── telegram.rs
│   │       │   ├── discord.rs
│   │       │   ├── github.rs
│   │       │   ├── arxiv.rs
│   │       │   └── hackernews.rs
│   │       ├── normalizer.rs           # Document normalization
│   │       ├── deduplicator.rs         # Deduplication
│   │       └── rate_limiter.rs         # Rate limiting
│   │
│   ├── extraction/                     # Extraction Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs
│   │       ├── chunker.rs              # Document chunking
│   │       ├── entity_extractor.rs
│   │       ├── claim_extractor.rs
│   │       ├── relationship_extractor.rs
│   │       ├── event_indicator.rs
│   │       └── merger.rs               # Dedup and merge
│   │
│   ├── embedding/                      # Embedding Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs
│   │       ├── embedder.rs
│   │       └── cache.rs
│   │
│   ├── correlation/                    # Correlation Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── event_engine/
│   │       │   ├── mod.rs
│   │       │   ├── matcher.rs
│   │       │   ├── formation.rs
│   │       │   ├── merger.rs
│   │       │   ├── scorer.rs
│   │       │   └── lifecycle.rs
│   │       ├── narrative_engine/
│   │       │   ├── mod.rs
│   │       │   ├── similarity.rs
│   │       │   ├── clustering.rs
│   │       │   ├── labeler.rs
│   │       │   ├── scorer.rs
│   │       │   └── lifecycle.rs
│   │       └── contradiction_engine/
│   │           ├── mod.rs
│   │           ├── candidate.rs
│   │           ├── detector.rs
│   │           ├── scorer.rs
│   │           ├── resolver.rs
│   │           └── lifecycle.rs
│   │
│   ├── broadcast/                      # Broadcast Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── scheduler.rs
│   │       ├── collector.rs
│   │       ├── prioritizer.rs
│   │       ├── generator.rs
│   │       ├── idle.rs                 # Idle content generation
│   │       ├── breaking.rs             # Breaking news handler
│   │       ├── formats/                # Output formats
│   │       │   ├── mod.rs
│   │       │   ├── brief.rs
│   │       │   ├── full.rs
│   │       │   └── audio_script.rs
│   │       └── history.rs
│   │
│   ├── audio/                          # Audio System
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── pipeline.rs
│   │       ├── script_parser.rs
│   │       ├── voice_manager.rs
│   │       ├── tts_engine.rs
│   │       ├── podcast.rs
│   │       ├── encoder.rs
│   │       ├── archiver.rs
│   │       ├── streamer.rs
│   │       └── assets.rs               # Intro/outro/transitions
│   │
│   ├── graph/                          # Knowledge Graph Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs
│   │       ├── connection.rs
│   │       ├── queries.rs
│   │       ├── mutations.rs
│   │       ├── schema.rs
│   │       ├── migration.rs
│   │       └── snapshot.rs
│   │
│   ├── model-runtime/                  # Model Runtime Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs
│   │       ├── llama.rs                # llama.cpp bindings
│   │       ├── onnx.rs                 # ONNX Runtime bindings
│   │       ├── whisper.rs              # whisper.cpp bindings
│   │       ├── queue.rs                # Inference queue
│   │       ├── router.rs               # Task routing
│   │       ├── registry.rs             # Model registry
│   │       └── downloader.rs           # Model download
│   │
│   ├── scheduler/                      # Scheduler Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs
│   │       └── job_store.rs
│   │
│   ├── api-gateway/                    # API Gateway Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs               # HTTP server
│   │       ├── routes/                 # Route handlers
│   │       │   ├── mod.rs
│   │       │   ├── events.rs
│   │       │   ├── narratives.rs
│   │       │   ├── entities.rs
│   │       │   ├── contradictions.rs
│   │       │   ├── sources.rs
│   │       │   ├── broadcasts.rs
│   │       │   ├── search.rs
│   │       │   ├── stats.rs
│   │       │   ├── health.rs
│   │       │   ├── config.rs
│   │       │   └── export.rs
│   │       ├── websocket.rs            # WebSocket handler
│   │       ├── middleware.rs           # Auth, logging, rate limiting
│   │       └── stream.rs               # Audio streaming
│   │
│   ├── plugin-host/                    # Plugin Host Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs
│   │       ├── discoverer.rs
│   │       ├── process_manager.rs
│   │       ├── connection_pool.rs
│   │       ├── event_bus.rs
│   │       └── validator.rs
│   │
│   ├── health/                         # Health Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs
│   │       ├── monitor.rs
│   │       ├── recovery.rs
│   │       └── alerting.rs
│   │
│   ├── store/                          # Document Store Service
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── service.rs
│   │       ├── archive.rs
│   │       ├── index.rs
│   │       └── retention.rs
│   │
│   ├── message-bus/                    # Message Bus abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── nats.rs                 # NATS implementation
│   │       └── memory.rs               # In-memory for testing
│   │
│   └── objective/                      # Main binary
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # Entry point
│           ├── app.rs                  # Application lifecycle
│           ├── config.rs               # Configuration loading
│           └── setup.rs                # First-run setup wizard
│
├── crates/ffi/                         # Foreign function interface
│   ├── llama-cpp/                      # Safe bindings to llama.cpp
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── context.rs
│   │       ├── inference.rs
│   │       └── model.rs
│   ├── onnx-runtime/                   # Safe bindings to ONNX Runtime
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── session.rs
│   │       └── tensor.rs
│   ├── whisper-cpp/                    # Safe bindings to whisper.cpp
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── context.rs
│   │       └── transcription.rs
│   └── kuzu/                           # Safe bindings to Kuzu DB
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── connection.rs
│           ├── database.rs
│           ├── query.rs
│           └── types.rs
│
├── bin/                                # Developer tooling
│   ├── objective-bench/                # Benchmarking tool
│   │   ├── Cargo.toml
│   │   └── src/main.rs
│   └── objective-test/                 # Integration test runner
│       ├── Cargo.toml
│       └── src/main.rs
│
├── dashboard/                          # Frontend application
│   ├── package.json
│   ├── tsconfig.json
│   ├── vite.config.ts
│   ├── tailwind.config.js
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── components/                 # Shared components
│       │   ├── layout/
│       │   │   ├── Sidebar.tsx
│       │   │   ├── Header.tsx
│       │   │   └── Footer.tsx
│       │   ├── common/
│       │   │   ├── Card.tsx
│       │   │   ├── Badge.tsx
│       │   │   ├── Gauge.tsx
│       │   │   ├── Timeline.tsx
│       │   │   ├── SearchBar.tsx
│       │   │   └── LoadingSpinner.tsx
│       │   ├── feed/
│       │   ├── events/
│       │   ├── narratives/
│       │   ├── graph/
│       │   ├── broadcasts/
│       │   ├── sources/
│       │   └── settings/
│       ├── pages/
│       │   ├── FeedPage.tsx
│       │   ├── EventsPage.tsx
│       │   ├── NarrativesPage.tsx
│       │   ├── GraphPage.tsx
│       │   ├── BroadcastsPage.tsx
│       │   ├── SourcesPage.tsx
│       │   └── SettingsPage.tsx
│       ├── hooks/
│       │   ├── useApi.ts
│       │   ├── useWebSocket.ts
│       │   └── useSearch.ts
│       ├── store/
│       │   ├── feedStore.ts
│       │   ├── eventStore.ts
│       │   └── uiStore.ts
│       ├── api/
│       │   ├── client.ts
│       │   └── types.ts
│       └── styles/
│           ├── globals.css
│           └── theme.ts
│
├── scripts/                            # Build & CI scripts
│   ├── build.sh
│   ├── test.sh
│   ├── lint.sh
│   ├── release.sh
│   ├── snapshot.sh                     # Snapshot creation
│   └── dev.sh                          # Dev environment setup
│
├── tests/                              # Integration tests
│   ├── common/
│   │   └── fixtures/                   # Test data
│   │       ├── rss-feed.xml
│   │       ├── reddit-post.json
│   │       ├── sample-document.txt
│   │       └── test-config.yaml
│   ├── ingestion/
│   │   ├── mock_source.rs
│   │   └── integration_test.rs
│   ├── extraction/
│   │   └── integration_test.rs
│   ├── correlation/
│   │   ├── event_engine_test.rs
│   │   ├── narrative_engine_test.rs
│   │   └── contradiction_engine_test.rs
│   ├── broadcast/
│   │   └── integration_test.rs
│   ├── api/
│   │   └── api_test.rs
│   └── end-to-end/
│       └── full_pipeline_test.rs
│
├── benchmarks/                         # Performance benchmarks
│   ├── ingestion_bench.rs
│   ├── extraction_bench.rs
│   └── graph_query_bench.rs
│
├── examples/                           # Configuration examples
│   ├── config/
│   │   ├── objective.full.yaml        # Full configuration reference
│   │   ├── objective.minimal.yaml     # Minimal configuration
│   │   └── sources.example.yaml       # Source configuration examples
│   ├── plugins/
│   │   └── example-plugin/
│   │       ├── Cargo.toml
│   │       ├── src/main.rs
│   │       └── plugin.json
│   └── prompts/
│       └── custom-extraction.yaml
│
├── .github/                            # GitHub configuration
│   ├── workflows/
│   │   ├── ci.yml                      # Continuous integration
│   │   ├── release.yml                 # Release automation
│   │   └── docs.yml                    # Documentation build
│   ├── CODEOWNERS                      # Code ownership
│   └── ISSUE_TEMPLATE/                 # Issue templates
│       ├── bug_report.md
│       ├── feature_request.md
│       └── config.yml
│
├── .gitignore
├── .pre-commit-config.yaml             # Pre-commit hooks
├── .editorconfig                       # Editor configuration
├── LICENSE
└── CONTRIBUTING.md                     # Contribution guidelines
```

## Interfaces

- `coding-standards.md` — coding standards for all services
- `docs/architecture/service-boundaries.md` — service definitions mapped to directories

## Failure Modes

| Failure | Impact | Mitigation |
|---------|--------|------------|
| Repository grows too large | Slow operations | Unrelated tooling in separate repos; large file storage (Git LFS) |
| Circular dependencies between crates | Compilation failure | Enforce dependency direction (core → services, never reverse) |
| Proto changes break services | Build failure | Backward-compatible protobuf changes; CI validates |
| Dashboard build divergence | Frontend/backend mismatch | Shared types in proto; contract tests |
| Large model files in repo | Repository bloated | Git LFS; keep models out of repo (download at setup) |

## Future Extensions

- Separate repositories for plugin SDK (for community plugins)
- API client library repository (for programmatic access)
- Helm chart repository (for Kubernetes deployment)
- Homebrew tap repository (for macOS distribution)
- Nix flake for reproducible builds
