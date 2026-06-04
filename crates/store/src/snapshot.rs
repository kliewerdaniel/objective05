use std::path::{Path, PathBuf};

use chrono::Utc;
use objective_core::{ObjectiveError, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Metadata about a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub created_at: String,
    pub events_file: Option<String>,
    pub scheduler_file: Option<String>,
    pub document_count: usize,
}

/// Creates timestamped snapshots of durable state files.
pub struct SnapshotService {
    state_dir: PathBuf,
    snapshot_root: PathBuf,
    document_root: PathBuf,
}

impl SnapshotService {
    pub fn new(data_root: &Path, state_dir: &Path, document_root: &Path) -> Self {
        Self {
            state_dir: state_dir.to_path_buf(),
            snapshot_root: data_root.join("snapshots"),
            document_root: document_root.to_path_buf(),
        }
    }

    /// Create a snapshot of all durable state. Returns the snapshot directory path.
    pub fn create_snapshot(&self) -> Result<PathBuf> {
        let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
        let snapshot_dir = self.snapshot_root.join(format!("snapshot-{timestamp}"));
        std::fs::create_dir_all(&snapshot_dir)
            .map_err(|e| ObjectiveError::Storage(format!("failed to create snapshot dir: {e}")))?;

        let mut manifest = SnapshotManifest {
            created_at: Utc::now().to_rfc3339(),
            events_file: None,
            scheduler_file: None,
            document_count: 0,
        };

        // Copy events file
        let events_path = self.state_dir.join("events.json");
        if events_path.exists() {
            let dest = snapshot_dir.join("events.json");
            std::fs::copy(&events_path, &dest)
                .map_err(|e| ObjectiveError::Storage(format!("failed to copy events file: {e}")))?;
            manifest.events_file = Some("events.json".to_string());
        }

        // Copy scheduler state
        let scheduler_path = self.state_dir.join("scheduler.jobstate");
        if scheduler_path.exists() {
            let dest = snapshot_dir.join("scheduler.jobstate");
            std::fs::copy(&scheduler_path, &dest).map_err(|e| {
                ObjectiveError::Storage(format!("failed to copy scheduler state: {e}"))
            })?;
            manifest.scheduler_file = Some("scheduler.jobstate".to_string());
        }

        // Count documents
        manifest.document_count = count_documents(&self.document_root)?;

        // Write manifest
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| ObjectiveError::Storage(format!("failed to serialize manifest: {e}")))?;
        std::fs::write(snapshot_dir.join("manifest.json"), &manifest_json)
            .map_err(|e| ObjectiveError::Storage(format!("failed to write manifest: {e}")))?;

        info!(
            snapshot_dir = %snapshot_dir.display(),
            events = manifest.events_file.is_some(),
            scheduler = manifest.scheduler_file.is_some(),
            documents = manifest.document_count,
            "snapshot created"
        );

        Ok(snapshot_dir)
    }

    /// List all existing snapshots, newest first.
    pub fn list_snapshots(&self) -> Result<Vec<SnapshotManifest>> {
        if !self.snapshot_root.exists() {
            return Ok(Vec::new());
        }

        let mut manifests = Vec::new();
        for entry in std::fs::read_dir(&self.snapshot_root)
            .map_err(|e| ObjectiveError::Storage(format!("failed to read snapshots dir: {e}")))?
        {
            let entry = entry.map_err(|e| {
                ObjectiveError::Storage(format!("failed to read snapshot entry: {e}"))
            })?;
            if !entry.file_type().map_err(storage_error)?.is_dir() {
                continue;
            }
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                let data = std::fs::read_to_string(&manifest_path).map_err(storage_error)?;
                if let Ok(manifest) = serde_json::from_str::<SnapshotManifest>(&data) {
                    manifests.push(manifest);
                }
            }
        }

        manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(manifests)
    }

    /// Restore state from a snapshot by copying files back to the state directory.
    pub fn restore_snapshot(&self, snapshot_dir: &Path) -> Result<()> {
        let manifest_path = snapshot_dir.join("manifest.json");
        if !manifest_path.exists() {
            return Err(ObjectiveError::Storage(
                "snapshot manifest not found".to_string(),
            ));
        }

        let data = std::fs::read_to_string(&manifest_path).map_err(storage_error)?;
        let manifest: SnapshotManifest = serde_json::from_str(&data).map_err(json_error)?;

        if let Some(events_file) = &manifest.events_file {
            let src = snapshot_dir.join(events_file);
            let dest = self.state_dir.join("events.json");
            std::fs::copy(&src, &dest).map_err(|e| {
                ObjectiveError::Storage(format!("failed to restore events file: {e}"))
            })?;
            info!("restored events.json from snapshot");
        }

        if let Some(scheduler_file) = &manifest.scheduler_file {
            let src = snapshot_dir.join(scheduler_file);
            let dest = self.state_dir.join("scheduler.jobstate");
            std::fs::copy(&src, &dest).map_err(|e| {
                ObjectiveError::Storage(format!("failed to restore scheduler state: {e}"))
            })?;
            info!("restored scheduler.jobstate from snapshot");
        }

        Ok(())
    }
}

fn count_documents(root: &Path) -> Result<usize> {
    if !root.exists() {
        return Ok(0);
    }

    let mut count = 0;
    for year_entry in std::fs::read_dir(root).map_err(storage_error)? {
        let year_entry = year_entry.map_err(storage_error)?;
        if !year_entry.file_type().map_err(storage_error)?.is_dir() {
            continue;
        }
        for month_entry in std::fs::read_dir(year_entry.path()).map_err(storage_error)? {
            let month_entry = month_entry.map_err(storage_error)?;
            if !month_entry.file_type().map_err(storage_error)?.is_dir() {
                continue;
            }
            for file_entry in std::fs::read_dir(month_entry.path()).map_err(storage_error)? {
                let file_entry = file_entry.map_err(storage_error)?;
                if file_entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".json.gz")
                {
                    count += 1;
                }
            }
        }
    }

    Ok(count)
}

fn storage_error(e: std::io::Error) -> ObjectiveError {
    ObjectiveError::Storage(format!("storage error: {e}"))
}

fn json_error(e: serde_json::Error) -> ObjectiveError {
    ObjectiveError::Storage(format!("json error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        tempfile::tempdir().unwrap().keep()
    }

    #[test]
    fn test_create_snapshot_copies_events_and_scheduler() {
        let dir = temp_dir();
        let state_dir = dir.join("state");
        let doc_dir = dir.join("documents");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::create_dir_all(&doc_dir).unwrap();

        // Create dummy state files
        std::fs::write(state_dir.join("events.json"), r#"[{"id":"test"}]"#).unwrap();
        std::fs::write(state_dir.join("scheduler.jobstate"), r#"{"jobs":{}}"#).unwrap();

        let service = SnapshotService::new(&dir, &state_dir, &doc_dir);
        let snapshot_dir = service.create_snapshot().unwrap();

        assert!(snapshot_dir.join("events.json").exists());
        assert!(snapshot_dir.join("scheduler.jobstate").exists());
        assert!(snapshot_dir.join("manifest.json").exists());

        let manifest_data = std::fs::read_to_string(snapshot_dir.join("manifest.json")).unwrap();
        let manifest: SnapshotManifest = serde_json::from_str(&manifest_data).unwrap();
        assert!(manifest.events_file.is_some());
        assert!(manifest.scheduler_file.is_some());
    }

    #[test]
    fn test_list_snapshots_returns_newest_first() {
        let dir = temp_dir();
        let state_dir = dir.join("state");
        let doc_dir = dir.join("documents");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::create_dir_all(&doc_dir).unwrap();

        let service = SnapshotService::new(&dir, &state_dir, &doc_dir);

        service.create_snapshot().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        service.create_snapshot().unwrap();

        let snapshots = service.list_snapshots().unwrap();
        assert_eq!(snapshots.len(), 2);
        assert!(snapshots[0].created_at >= snapshots[1].created_at);
    }

    #[test]
    fn test_restore_snapshot_copies_files_back() {
        let dir = temp_dir();
        let state_dir = dir.join("state");
        let doc_dir = dir.join("documents");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::create_dir_all(&doc_dir).unwrap();

        std::fs::write(state_dir.join("events.json"), r#"[{"id":"original"}]"#).unwrap();

        let service = SnapshotService::new(&dir, &state_dir, &doc_dir);
        let snapshot_dir = service.create_snapshot().unwrap();

        // Overwrite original
        std::fs::write(state_dir.join("events.json"), r#"[{"id":"modified"}]"#).unwrap();

        // Restore
        service.restore_snapshot(&snapshot_dir).unwrap();

        let restored = std::fs::read_to_string(state_dir.join("events.json")).unwrap();
        assert!(restored.contains("original"));
        assert!(!restored.contains("modified"));
    }
}
