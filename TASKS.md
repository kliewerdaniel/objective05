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
- `[x]` Implement event, narrative, contradiction, broadcast, audio, scheduler, plugin host, health, and storage manager services.
  - `[x]` Scheduler service (cron parsing, job definitions, state persistence, event emission)
  - `[x]` Pipeline worker (event-driven ingestion, extraction, event engine correlation, maintenance, and snapshot; background task on startup)
  - `[x]` Plugin host crate (`objective-plugin-host`) with lifecycle states, manifest discovery, bus event routing, restart/health, persistent state, and `audit-log` + `re-emitter` built-ins.
- `[ ]` Implement model-runtime-backed extraction using local llama.cpp and ONNX.
  - `[x]` Design the v1 surface in `docs/processing/model-runtime.md` and `ADR-015`: `ModelRuntime` trait in `objective-core::traits`, `NoopRuntime` provider, `RuntimeExtractionService` orchestrator with per-chunk fallback to the heuristic, and `ModelRuntimeConfig::{Disabled, Heuristic, Local}` on `ObjectiveConfig`. Default `Disabled` preserves v0 behaviour.
  - `[x]` Phase 1 — land the trait + orchestrator: `ModelRuntime` / `InferenceTask` / `InferenceResult` / `ModelError` in `objective-core::traits`; `crates/model-runtime` with `NoopRuntime::{heuristic, passthrough}`; `RuntimeExtractionService` in `crates/extraction`; `ObjectiveConfig::model_runtime` plumbing; feature-gated wiring in `crates/objective`; tests for fallback paths and chunk-level error tolerance.
  - `[x]` Phase 2 — add `ort` (ONNX Runtime) behind the `onnx` feature; implement `InferenceKind::Embedding`; wire `bge-small-en-v1.5` lookup; attach a `vector_index: ModelIndex` sidecar to `ExtractionResult` for the existing `LanceDB` stub. Phase 2 ships the deterministic hash-based stub; the real `ort::Session` path lands in Phase 2.5.
  - `[ ]` Phase 3 — add `llama-cpp-rs` behind the `llama` feature; implement `NER` / `ClaimExtraction` / `RelationExtraction`; ship the default prompt set in `crates/extraction/src/prompts/`; per-task routing via the `default_strategy` block; `POST /api/v1/model-runtime/reload` for hot model load.
  - `[ ]` Phase 4 — model state machine exposed via `/api/v1/model-runtime`; per-model inference queue (default 4 concurrent); per-model timeout enforcement at the runtime level; latency histograms surfaced through the existing monitoring service.

## APIs

- `[x]` Implement API gateway crate.
- `[x]` Implement `GET /api/v1/health`.
- `[x]` Implement `GET /api/v1/stats`.
- `[x]` Implement `GET /api/v1/documents`, `GET /api/v1/extractions`, and `GET /api/v1/events`.
- `[x]` Implement `GET /api/v1/entities`, `GET /api/v1/entities/summary`, and `GET /api/v1/claims`.
- `[x]` Implement `GET /api/v1/sources`, `GET /api/v1/narratives`, `GET /api/v1/contradictions`, and `GET /api/v1/broadcasts`.
- `[x]` Implement `GET /api/v1/derived-events/:id` and `GET /api/v1/entities/:name` detail endpoints.
- `[x]` Add OpenAPI annotations and a `GET /api-docs/openapi.json` endpoint that exposes the full schema.
- `[x]` Implement `GET /api/v1/recovery` and `POST /api/v1/recovery/check` recovery state and force-check endpoints.
- `[x]` Add a `WebSocketHub` that bridges the message bus to a `/ws` WebSocket endpoint with per-connection channel filtering and a documented welcome/event/pong/error wire protocol.
- `[x]` Add HTTP route tests for implemented API endpoints.
- `[x]` Add WebSocket integration tests covering the 503 fallback and the live event stream.
- `[x]` Implement `SourceRegistry` with persisted CRUD and a `Box<dyn SourceAdapter>` factory per source type.
- `[x]` Expose source registry CRUD through `GET/POST /api/v1/source-registry`, `GET/PUT/DELETE /api/v1/source-registry/:name`, and `POST /api/v1/source-registry/:name/trigger`.
- `[x]` Wire the registry into `AppState` (seeded with `hackernews_front` + `lobsters` on first boot) and attach to `ApiState` so the dashboard can manage live adapters.
- `[x]` Add integration tests for the registry routes (503 unconfigured, full CRUD round-trip, duplicate 409, missing 404).
- `[x]` Implement remaining documented REST endpoints:
  - `[x]` `GET /api/v1/narratives/:id` (read a single narrative, 404 on miss)
  - `[x]` `GET /api/v1/broadcasts/latest` and `GET /api/v1/broadcasts/:id` (latest + read)
  - `[x]` `POST /api/v1/broadcasts/generate` (on-demand broadcast draft, optional `{ title, focus }`)
  - `[x]` `POST /api/v1/contradictions/:id/resolve` (mark resolved with optional status + note)
  - `[x]` `POST /api/v1/events/:id/resolve` (set `EventStatus::Resolved`, persist via `EventRepository`)
  - `[x]` `POST /api/v1/entities/merge` (rewrite subject/object/from/to, drop source entity)
  - `[x]` `GET /api/v1/search?q=&limit=` (substring search across documents, entities, claims)
  - `[x]` `GET /api/v1/export` (JSON download with documents, entity/claim/relationship summaries)
  - `[x]` `GET /api/v1/config` (flattened live configuration; 503 when not attached)
  - `[x]` OpenAPI schemas + tags updated; integration tests cover every new endpoint.
- `[x]` Implement plugin host routes:
  - `[x]` `GET /api/v1/plugins` and `GET /api/v1/plugins/:name`
  - `[x]` `POST /api/v1/plugins/:name/restart` (force restart, increments restart count)
  - `[x]` `POST /api/v1/plugins/reload` (re-validate the registry)
  - `[x]` OpenAPI schemas + tag added; integration tests for the 503 fallback, list, get, restart, and reload paths.

## User Interface

- `[x]` Scaffold documented dashboard project.
  - Vite + React 19 + TypeScript workspace under `dashboard/`.
  - `vite.config.ts` proxies `/api` and `/ws` to the running Rust daemon.
  - `index.html` branded as "Objective — Local-First Intelligence OS".
- `[x]` Implement feed, events, narratives, graph, broadcasts, sources, and settings pages.
  - `FeedPage` — searchable document list with source filter and detail
    drawer handoff.
  - `EventsPage` — derived-event grid with importance bar, type filter,
    sort toggle, and one-click "Resolve" action.
  - `NarrativesPage` — narrative / contradiction sub-tabs with resolve
    action and severity badge.
  - `GraphPage` — canvas-based force-directed graph with drag, pan,
    zoom, search highlight, and per-node detail panel.
  - `BroadcastsPage` — broadcast archive list + transcript reader with
    audio-player widget and Generate Now action.
  - `SourcesPage` — SourceRegistry CRUD UI (add / patch / delete /
    trigger) with source-type-aware URL validation.
  - `SettingsPage` — model registry, broadcast schedule, and live
    configuration + dataset export.
  - `DetailDrawer` — slide-over details pane for documents, events,
    and entities.
  - `Header` / `Footer` / `Sidebar` — live WebSocket status, uptime
    widget, sync button, and metric ticker.
- `[x]` Add responsive, accessible component tests and Playwright coverage.
  - Vitest + React Testing Library + jsdom installed
    (`vitest.config.mjs`, `tsconfig.test.json`, `src/test/setup.ts`).
  - 46 unit + component tests across the API client, both stores, the
    `useWebSocket` hook, the `Header` and `Sidebar` layout components,
    and the `DetailDrawer`. Run with `cd dashboard && npm test`;
    `npm run test:watch` for iterative work; `npm run test:types` to
    typecheck the test files against `tsconfig.test.json`.
  - The test suite stubs the API client and WebSocket so no daemon is
    required to execute it.
  - Playwright end-to-end coverage remains a follow-up; the unit tests
    exercise the same render paths the E2E suite would target.
  - Dashboard builds cleanly: `npm run build` → ~303 kB JS / ~6 kB CSS
    (84 kB / 2 kB gzipped).

## Background Systems

- `[x]` Implement scheduler job store and periodic trigger flow.
  - `[x]` CronSchedule parser (5-field cron expressions)
  - `[x]` JobDefinition and JobState types
  - `[x]` SchedulerService with event emission via MessageBus
  - `[x]` Default job configuration (RSS, extraction, broadcast, maintenance, snapshot)
  - `[x]` Manual trigger and enable/disable support
  - `[x]` Persistent job state storage (JSON file at `.objective/state/scheduler.jobstate`)
  - [x] Integration with main application binary (background task on startup)
- `[x]` Implement durable queue retry and dead-letter behavior.
- `[x]` Implement monitoring hooks and recovery service.
  - `[x]` RecoveryService observes MonitoringService metrics
  - `[x]` Detects stalled and erroring pipelines and publishes `system.service.crash`
  - `[x]` Publishes `system.service.recovered` on return to healthy state
  - `[x]` Publishes periodic `system.heartbeat` events
  - `[x]` Persistent recovery state across restarts
  - `[x]` `GET /api/v1/recovery` and `POST /api/v1/recovery/check` routes
  - `[x]` Wired into the running daemon as a background task


## Testing

- `[x]` Add unit tests for config defaults, memory bus, memory store, normalizer, ingestion, and extraction.
- `[x]` Add integration test for RSS ingestion to extraction pipeline.
- `[x]` Add integration tests for seeded ingestion/extraction visibility through HTTP routes.
- `[x]` Add integration tests for full end-to-end ingestion to extraction to API flow with durable storage.
- [x] Add scheduler and full pipeline regression suites.

## Documentation

- `[x]` Add minimal runnable implementation notes to README.
- `[x]` Add minimal configuration example.
- `[x]` Update architecture notes as storage and model-runtime implementations replace MVP in-memory components.
- `[x]` Add deployment instructions for packaged binaries and Docker verification.
  - New `docs/deployment/from-source.md` covers prerequisites, local
    development, Docker build/run/verify, and candidate production
    bundle assembly.
  - `installation.md` cross-references the new doc and continues to
    describe the long-term packaged-binary flows.

## Assumptions

- The first vertical slice uses an in-memory message bus and in-memory extraction storage while preserving the documented trait boundaries for NATS, Kuzu, and LanceDB.
- Raw documents now persist to gzip JSON files under the configured document archive path.
- The first extractor is heuristic and schema-compatible. It is a temporary provider behind `DocumentProcessor` until the model runtime is implemented.
- Local data paths default to `.objective` inside the repository for development; packaged builds should use the documented user data directory.
- The `RecoveryService` is a watchdog for the ingestion/extraction pipeline. It observes `MonitoringService` metrics, publishes `system.service.crash` when both pipeline activity has stalled and the error count has climbed, and publishes `system.service.recovered` when the pipeline returns to a healthy state. Heartbeats (`system.heartbeat`) are emitted on a configurable cadence.
- The dashboard's `/ws` endpoint bridges the bus to the UI through a polling `WebSocketHub`. The hub uses a 250 ms poll cadence and a 1024-message broadcast channel; clients filter by logical channel (`events`, `broadcast`, `system`). The wire format mirrors `docs/api/internal-api.md`.
- The `SourceRegistry` is the source of truth for live ingestion adapters. It persists to `.objective/state/sources.json` and seeds `hackernews_front` (hnrss frontpage) and `lobsters` on first boot. Adapter instances are constructed on demand via `SourceRegistry::spawn_adapter`, returning `Box<dyn SourceAdapter>` for heterogeneous dispatch.
- `AuxiliaryStores` (narrative, broadcast, contradiction) live inside the api-gateway under `routes/auxiliary.rs` rather than in `objective-core`. They are API-only state with no cross-crate consumers, kept behind `Arc<RwLock<HashMap<Ulid, _>>>` and bundled into `ApiState` so they can be replaced with a durable implementation later without touching downstream crates. The `POST /api/v1/broadcasts/generate` endpoint produces a `BroadcastStatus::Draft` record with a stub markdown body; a real generator is left for follow-up.
- The plugin host v1 is fully in-process: built-in plugins are `Arc<dyn Plugin>` objects registered at startup, and discovered manifests under `.objective/plugins/<name>/plugin.json` are mounted as `NoopPlugin` placeholders so they show up in the API and counts. The v1 does not yet spawn external plugin processes or speak the gRPC contract from `docs/api/plugin-api.md`; the API surface and lifecycle states mirror the spec so the upgrade path is mechanical. Built-in plugins shipped today are `audit-log` (records every event the host routes to it) and `re-emitter` (republishes `extraction.document.processed` events onto `plugin.re_emitted`). Both run on the live daemon via the `objective` binary's background task.
- The dashboard is a Vite + React 19 + TypeScript SPA under `dashboard/`. The Vite dev server proxies `/api` and `/ws` to the running Rust daemon on `127.0.0.1:8080`, so the SPA always talks to localhost regardless of where it is served from. The production build emits a static `dist/` bundle (~85 kB gzipped JS, ~2 kB gzipped CSS) that the Rust gateway can serve directly from a packaged build. Tests run with Vitest + React Testing Library + jsdom (46 tests across the API client, both stores, the `useWebSocket` hook, the `Header` and `Sidebar` layout components, and the `DetailDrawer`). Run with `cd dashboard && npm test`; `npm run test:types` typechecks the test files; the suite stubs the API client and WebSocket so no daemon is required.
