//! Storage trait and implementations for events.
//!
//! The trait mirrors the shape of `KuzuGraphStore` so the runtime can
//! later swap the in-memory implementation for a real graph store
//! without touching the rest of the engine.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use objective_core::{types::Event, Result};
use tokio::sync::RwLock;
use tracing::warn;

/// Storage abstraction for derived [`Event`] records.
#[async_trait]
pub trait EventRepository: Send + Sync {
    /// Persist a new or updated event.
    async fn save_event(&self, event: Event) -> Result<()>;

    /// Fetch every event currently known to the repository.
    async fn list_events(&self) -> Result<Vec<Event>>;

    /// Fetch a single event by its ULID identifier.
    async fn get_event(&self, id: &str) -> Result<Option<Event>>;

    /// Replace every event in the repository with the provided list.
    /// Used by periodic maintenance to persist status transitions in a
    /// single call.
    async fn replace_all(&self, events: Vec<Event>) -> Result<()>;
}

/// Thread-safe in-memory event repository, used by the runtime and the
/// test suite. Behaviour matches the documented contract: the most
/// recent write wins, and `list_events` returns everything currently
/// in memory sorted by last-updated descending.
#[derive(Debug, Default)]
pub struct InMemoryEventRepository {
    events: RwLock<HashMap<String, Event>>,
}

impl InMemoryEventRepository {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_events(events: Vec<Event>) -> Self {
        let mut map = HashMap::new();
        for event in events {
            map.insert(event.id.to_string(), event);
        }
        Self {
            events: RwLock::new(map),
        }
    }
}

#[async_trait]
impl EventRepository for InMemoryEventRepository {
    async fn save_event(&self, event: Event) -> Result<()> {
        let mut guard = self.events.write().await;
        guard.insert(event.id.to_string(), event);
        Ok(())
    }

    async fn list_events(&self) -> Result<Vec<Event>> {
        let guard = self.events.read().await;
        let mut events: Vec<Event> = guard.values().cloned().collect();
        events.sort_by(|a, b| b.last_updated_at.cmp(&a.last_updated_at));
        Ok(events)
    }

    async fn get_event(&self, id: &str) -> Result<Option<Event>> {
        let guard = self.events.read().await;
        Ok(guard.get(id).cloned())
    }

    async fn replace_all(&self, events: Vec<Event>) -> Result<()> {
        let mut map = HashMap::new();
        for event in events {
            map.insert(event.id.to_string(), event);
        }
        *self.events.write().await = map;
        Ok(())
    }
}

/// File-backed event repository that persists events as a JSON file.
///
/// Events are loaded from disk on first access and flushed after every
/// write. The on-disk format is a JSON array of `Event` objects.
pub struct FileEventRepository {
    path: PathBuf,
    events: RwLock<HashMap<String, Event>>,
}

impl FileEventRepository {
    /// Create a new file-backed repository. If the file exists it will be
    /// loaded; otherwise an empty store is created.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let events = Self::load_from_file(&path);
        Self {
            path,
            events: RwLock::new(events),
        }
    }

    fn load_from_file(path: &Path) -> HashMap<String, Event> {
        if !path.exists() {
            return HashMap::new();
        }

        let data = match std::fs::read_to_string(path) {
            Ok(data) => data,
            Err(e) => {
                warn!(?e, path = %path.display(), "failed to read event file");
                return HashMap::new();
            }
        };

        if data.trim().is_empty() {
            return HashMap::new();
        }

        let events: Vec<Event> = match serde_json::from_str(&data) {
            Ok(events) => events,
            Err(e) => {
                warn!(?e, path = %path.display(), "failed to parse event file");
                return HashMap::new();
            }
        };

        let mut map = HashMap::new();
        for event in events {
            map.insert(event.id.to_string(), event);
        }
        map
    }

    async fn flush_to_file(&self) -> Result<()> {
        let guard = self.events.read().await;
        let events: Vec<&Event> = guard.values().collect();
        let json = serde_json::to_string_pretty(&events).map_err(|e| {
            objective_core::ObjectiveError::Storage(format!("failed to serialize events: {e}"))
        })?;
        drop(guard);

        // Write to a temp file then rename for atomicity
        let tmp_path = self.path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json).map_err(|e| {
            objective_core::ObjectiveError::Storage(format!(
                "failed to write event file {}: {e}",
                tmp_path.display()
            ))
        })?;
        std::fs::rename(&tmp_path, &self.path).map_err(|e| {
            objective_core::ObjectiveError::Storage(format!(
                "failed to rename event file {}: {e}",
                self.path.display()
            ))
        })?;

        Ok(())
    }
}

#[async_trait]
impl EventRepository for FileEventRepository {
    async fn save_event(&self, event: Event) -> Result<()> {
        {
            let mut guard = self.events.write().await;
            guard.insert(event.id.to_string(), event);
        }
        self.flush_to_file().await
    }

    async fn list_events(&self) -> Result<Vec<Event>> {
        let guard = self.events.read().await;
        let mut events: Vec<Event> = guard.values().cloned().collect();
        events.sort_by(|a, b| b.last_updated_at.cmp(&a.last_updated_at));
        Ok(events)
    }

    async fn get_event(&self, id: &str) -> Result<Option<Event>> {
        let guard = self.events.read().await;
        Ok(guard.get(id).cloned())
    }

    async fn replace_all(&self, events: Vec<Event>) -> Result<()> {
        {
            let mut map = HashMap::new();
            for event in events {
                map.insert(event.id.to_string(), event);
            }
            *self.events.write().await = map;
        }
        self.flush_to_file().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use objective_core::types::{EventStatus, EventType};
    use std::collections::HashMap;

    fn make_event(id: &str, title: &str) -> Event {
        Event {
            id: id.parse().unwrap(),
            title: title.to_string(),
            description: format!("Description of {title}"),
            event_type: EventType::Technology,
            status: EventStatus::Active,
            importance: 0.5,
            confidence: 0.5,
            source_diversity: 0.0,
            claim_count: 1,
            participating_entities: vec!["Entity".to_string()],
            location: None,
            first_observed_at: Utc::now(),
            last_updated_at: Utc::now(),
            last_claim_at: Utc::now(),
            claim_ids: vec![],
            document_ids: vec![],
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_in_memory_save_and_list() {
        let repo = InMemoryEventRepository::new();
        repo.save_event(make_event("01HQ1Y2Z3A4B5C6D7E8F9G0H1J", "Event A"))
            .await
            .unwrap();
        repo.save_event(make_event("01HQ1Y2Z3A4B5C6D7E8F9G0H2K", "Event B"))
            .await
            .unwrap();

        let events = repo.list_events().await.unwrap();
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn test_file_repo_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.json");

        {
            let repo = FileEventRepository::new(&path);
            repo.save_event(make_event("01HQ1Y2Z3A4B5C6D7E8F9G0H1J", "Persisted A"))
                .await
                .unwrap();
            repo.save_event(make_event("01HQ1Y2Z3A4B5C6D7E8F9G0H2K", "Persisted B"))
                .await
                .unwrap();
        }

        // Re-open from disk
        let repo2 = FileEventRepository::new(&path);
        let events = repo2.list_events().await.unwrap();
        assert_eq!(events.len(), 2);
        assert!(events.iter().any(|e| e.title == "Persisted A"));
        assert!(events.iter().any(|e| e.title == "Persisted B"));
    }

    #[tokio::test]
    async fn test_file_repo_replace_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.json");

        let repo = FileEventRepository::new(&path);
        repo.save_event(make_event("01HQ1Y2Z3A4B5C6D7E8F9G0H1J", "Original"))
            .await
            .unwrap();

        repo.replace_all(vec![make_event("01HQ1Y2Z3A4B5C6D7E8F9G0H2K", "Replaced")])
            .await
            .unwrap();

        let events = repo.list_events().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Replaced");
    }
}
