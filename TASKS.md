# Objective05 Tasks

## Status Legend

- `[x]` completed
- `[~]` in progress
- `[ ]` pending
- `[!]` blocked or deferred

## Foundation

- `[x]` Create Rust Cargo workspace matching documented service boundaries.
- `[x]` Add Rust toolchain, Makefile, Dockerfile, gitignore, and local config example.
- `[x]` Implement configuration loading with file and `OBJECTIVE__...` environment overrides.
- `[x]` Implement tracing initialization.
- `[x]` Add setup command to initialize local data directories.
- `[x]` Wire the Kuzu and LanceDB stores into the runtime app behind trait-stable in-memory stubs that preserve the documented `GraphRepository` and `VectorRepository` interfaces.
- `[x]` Promote the rust-toolchain pin and workspace dependencies to versions that compile under the current Cargo resolver.

## Data Layer

- `[x]` Define core typed schemas for raw documents, event envelopes, sources, entities, claims, relationships, and extraction results.
- `[x]` Implement in-memory document and extraction repositories for MVP and tests.
- `[x]` Implement Kuzu graph store trait surface (in-memory stub, real Kuzu integration pending schema migration work).
- `[x]` Implement LanceDB vector store trait surface (in-memory stub with cosine similarity, real LanceDB integration pending schema migration work).
- `[x]` Implement compressed document archive partitioned by year/month.
- `[x]` Implement snapshot and backup operations.
- `[x]` Implement durable event repository (FileEventRepository, persists to JSON on disk).

## Core Services

- `[x]` Implement source adapter trait.
- `[x]` Implement document normalizer with HTML stripping, validation, truncation, language defaulting, and SHA-256 hashing.
- `[x]` Implement ingestion service that stores documents and publishes documented ingestion events.
- `[x]` Implement first extraction processor returning documented entity, claim, and relationship structures.
- `[x]` Implement RSS adapter with URL polling, RSS/Atom parsing, metadata preservation, cursor filtering, and offline fixture tests.
- `[x]` Implement Hacker News adapter backed by the public Algolia search API with offline JSON fixture tests.
- `[x]` Implement Reddit, YouTube, podcast, PDF, SEC, government feed, web, email, Telegram, Discord, GitHub, and arXiv adapters.
  - `[x]` Reddit adapter (public JSON API, subreddit posts)
  - `[x]` YouTube adapter (channel RSS/Atom feeds)
  - `[x]` GitHub adapter (public events API)
  - `[x]` arXiv adapter (Atom API, academic papers)
  - `[x]` Web adapter (generic HTML scraper)
  - `[x]` Podcast adapter (RSS with audio metadata, enclosure URL, duration, episode/season tags)
  - `[x]` SEC EDGAR adapter (full-text search API, financial filings)
  - `[x]` GitHub Releases adapter (releases API, tags, authors, assets)
- `[ ]` Implement model-runtime-backed extraction using local llama.cpp and ONNX.
- `[x]` Implement event, narrative, contradiction, broadcast, audio, scheduler, plugin host, health, and storage manager services.
  - `[x]` Scheduler service (cron parsing, job definitions, state persistence, event emission)
  - `[x]` Pipeline worker (event-driven ingestion, extraction, event engine correlation, maintenance, and snapshot; background task on startup)

## APIs

- `[x]` Implement API gateway crate.
- `[x]` Implement `GET /api/v1/health`.
- `[x]` Implement `GET /api/v1/stats`.
- `[x]` Implement `GET /api/v1/documents`, `GET /api/v1/extractions`, and `GET /api/v1/events`.
- `[x]` Implement `GET /api/v1/entities`, `GET /api/v1/entities/summary`, and `GET /api/v1/claims`.
- `[x]` Implement `GET /api/v1/sources`, `GET /api/v1/narratives`, `GET /api/v1/contradictions`, and `GET /api/v1/broadcasts`.
- `[x]` Implement `GET /api/v1/derived-events/:id` and `GET /api/v1/entities/:name` detail endpoints.
- `[x]` Add OpenAPI annotations and a `GET /api-docs/openapi.json` endpoint that exposes the full schema.
- `[ ]` Implement remaining documented REST endpoints and WebSocket channels.
- `[x]` Add HTTP route tests for implemented API endpoints.
- `[ ]` Add API contract and integration tests for all remaining endpoints.

## User Interface

- `[ ]` Scaffold documented dashboard project.
- `[ ]` Implement feed, events, narratives, graph, broadcasts, sources, and settings pages.
- `[ ]` Add responsive, accessible component tests and Playwright coverage.

## Background Systems

- `[ ]` Implement scheduler job store and periodic trigger flow.
  - `[x]` CronSchedule parser (5-field cron expressions)
  - `[x]` JobDefinition and JobState types
  - `[x]` SchedulerService with event emission via MessageBus
  - `[x]` Default job configuration (RSS, extraction, broadcast, maintenance, snapshot)
  - `[x]` Manual trigger and enable/disable support
  - `[x]` Persistent job state storage (JSON file at `.objective/state/scheduler.jobstate`)
  - [x] Integration with main application binary (background task on startup)
- `[x]` Implement durable queue retry and dead-letter behavior.
- `[ ]` Implement monitoring hooks and recovery service.

## Testing

- `[x]` Add unit tests for config defaults, memory bus, memory store, normalizer, ingestion, and extraction.
- `[x]` Add integration test for RSS ingestion to extraction pipeline.
- `[x]` Add integration tests for seeded ingestion/extraction visibility through HTTP routes.
- `[x]` Add integration tests for full end-to-end ingestion to extraction to API flow with durable storage.
- [x] Add scheduler and full pipeline regression suites.

## Documentation

- `[x]` Add minimal runnable implementation notes to README.
- `[x]` Add minimal configuration example.
- `[ ]` Update architecture notes as storage and model-runtime implementations replace MVP in-memory components.
- `[ ]` Add deployment instructions for packaged binaries and Docker verification.

## Assumptions

- The first vertical slice uses an in-memory message bus and in-memory extraction storage while preserving the documented trait boundaries for NATS, Kuzu, and LanceDB.
- Raw documents now persist to gzip JSON files under the configured document archive path.
- The first extractor is heuristic and schema-compatible. It is a temporary provider behind `DocumentProcessor` until the model runtime is implemented.
- Local data paths default to `.objective` inside the repository for development; packaged builds should use the documented user data directory.
