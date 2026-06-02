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
- `[~]` Replace MVP in-memory dependencies with documented embedded NATS, Kuzu, and LanceDB integrations.

## Data Layer

- `[x]` Define core typed schemas for raw documents, event envelopes, sources, entities, claims, relationships, and extraction results.
- `[x]` Implement in-memory document and extraction repositories for MVP and tests.
- `[ ]` Implement Kuzu graph schema, migrations, and repository queries.
- `[ ]` Implement LanceDB vector storage and embedding cache.
- `[x]` Implement compressed document archive partitioned by year/month.
- `[ ]` Implement snapshot and backup operations.

## Core Services

- `[x]` Implement source adapter trait.
- `[x]` Implement document normalizer with HTML stripping, validation, truncation, language defaulting, and SHA-256 hashing.
- `[x]` Implement ingestion service that stores documents and publishes documented ingestion events.
- `[x]` Implement first extraction processor returning documented entity, claim, and relationship structures.
- `[x]` Implement RSS adapter with URL polling, RSS/Atom parsing, metadata preservation, cursor filtering, and offline fixture tests.
- `[ ]` Implement Reddit, YouTube, podcast, PDF, SEC, government feed, web, email, Telegram, Discord, GitHub, arXiv, and Hacker News adapters.
- `[ ]` Implement model-runtime-backed extraction using local llama.cpp and ONNX.
- `[ ]` Implement event, narrative, contradiction, broadcast, audio, scheduler, plugin host, health, and storage manager services.

## APIs

- `[x]` Implement API gateway crate.
- `[x]` Implement `GET /api/v1/health`.
- `[x]` Implement `GET /api/v1/stats`.
- `[x]` Implement `GET /api/v1/documents`, `GET /api/v1/extractions`, and `GET /api/v1/events`.
- `[~]` Add OpenAPI annotations for implemented endpoints.
- `[ ]` Implement remaining documented REST endpoints and WebSocket channels.
- `[x]` Add HTTP route tests for implemented API endpoints.
- `[ ]` Add API contract and integration tests for all remaining endpoints.

## User Interface

- `[ ]` Scaffold documented dashboard project.
- `[ ]` Implement feed, events, narratives, graph, broadcasts, sources, and settings pages.
- `[ ]` Add responsive, accessible component tests and Playwright coverage.

## Background Systems

- `[ ]` Implement scheduler job store and periodic trigger flow.
- `[ ]` Implement durable queue retry and dead-letter behavior.
- `[ ]` Implement monitoring hooks and recovery service.

## Testing

- `[x]` Add unit tests for config defaults, memory bus, memory store, normalizer, ingestion, and extraction.
- `[x]` Add integration test for RSS ingestion to extraction pipeline.
- `[x]` Add integration tests for seeded ingestion/extraction visibility through HTTP routes.
- `[ ]` Add integration tests for full end-to-end ingestion to extraction to API flow with durable storage.
- `[ ]` Add graph, vector, API, scheduler, and broadcast regression suites.

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
