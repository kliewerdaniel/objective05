//! Event engine crate for the Objective platform.
//!
//! The engine is responsible for turning extracted claims into
//! deduplicated, importance-scored events. It mirrors the pipeline
//! described in `docs/processing/event-engine.md` and is implemented
//! against the documented [`objective_core::types::Event`] and
//! [`objective_core::types::FirstClaim`] types so the storage layer can
//! later promote the in-memory store to Kuzu without changing
//! downstream consumers.

pub mod engine;
pub mod store;
pub mod titles;

pub use engine::{EventEngine, EventEngineConfig, IngestOutcome};
pub use store::{EventRepository, FileEventRepository, InMemoryEventRepository};
pub use titles::{generate_event_description, generate_event_title, infer_event_type};
