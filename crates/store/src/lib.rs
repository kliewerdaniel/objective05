pub mod kuzu;
pub mod vectordb;

use std::{
    collections::HashSet,
    fs::File,
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use chrono::Datelike;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use objective_core::{
    traits::{DocumentRepository, ExtractionRepository},
    types::{ExtractionResult, RawDocument},
    ObjectiveError, Result,
};
use tokio::sync::RwLock;

#[derive(Debug, Default)]
pub struct InMemoryStore {
    documents: RwLock<Vec<RawDocument>>,
    extractions: RwLock<Vec<ExtractionResult>>,
}

#[derive(Debug)]
pub struct DocumentArchive {
    root: PathBuf,
}

impl DocumentArchive {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn document_path(&self, document: &RawDocument) -> PathBuf {
        self.partition_path(document.fetched_at.year(), document.fetched_at.month())
            .join(format!("{}.json.gz", document.id))
    }

    fn partition_path(&self, year: i32, month: u32) -> PathBuf {
        self.root
            .join(format!("{year:04}"))
            .join(format!("{month:02}"))
    }

    fn all_document_paths(&self) -> Result<Vec<PathBuf>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }

        let mut paths = Vec::new();
        for year_entry in std::fs::read_dir(&self.root).map_err(storage_error)? {
            let year_entry = year_entry.map_err(storage_error)?;
            if !year_entry.file_type().map_err(storage_error)?.is_dir() {
                continue;
            }

            for month_entry in std::fs::read_dir(year_entry.path()).map_err(storage_error)? {
                let month_entry = month_entry.map_err(storage_error)?;
                if !month_entry.file_type().map_err(storage_error)?.is_dir() {
                    continue;
                }

                for document_entry in
                    std::fs::read_dir(month_entry.path()).map_err(storage_error)?
                {
                    let document_entry = document_entry.map_err(storage_error)?;
                    let path = document_entry.path();
                    if path.extension().and_then(|extension| extension.to_str()) == Some("gz") {
                        paths.push(path);
                    }
                }
            }
        }

        paths.sort();
        Ok(paths)
    }

    fn read_document(path: &Path) -> Result<RawDocument> {
        let file = File::open(path).map_err(storage_error)?;
        let decoder = GzDecoder::new(BufReader::new(file));
        serde_json::from_reader(decoder).map_err(|error| {
            ObjectiveError::Storage(format!(
                "failed to read document archive {}: {error}",
                path.display()
            ))
        })
    }

    fn write_document(path: &Path, document: &RawDocument) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(storage_error)?;
        }

        let temp_path = path.with_extension("json.gz.tmp");
        let file = File::create(&temp_path).map_err(storage_error)?;
        let encoder = GzEncoder::new(BufWriter::new(file), Compression::default());
        serde_json::to_writer(encoder, document).map_err(|error| {
            ObjectiveError::Storage(format!("failed to serialize document: {error}"))
        })?;
        std::fs::rename(&temp_path, path).map_err(storage_error)?;
        Ok(())
    }
}

#[async_trait]
impl DocumentRepository for DocumentArchive {
    async fn save_document(&self, document: RawDocument) -> Result<()> {
        let documents = self.list_documents().await?;
        if documents.iter().any(|existing| {
            existing.content_hash == document.content_hash || existing.url == document.url
        }) {
            return Ok(());
        }

        Self::write_document(&self.document_path(&document), &document)
    }

    async fn list_documents(&self) -> Result<Vec<RawDocument>> {
        let mut documents = Vec::new();
        let mut seen_hashes = HashSet::new();
        for path in self.all_document_paths()? {
            let document = Self::read_document(&path)?;
            if seen_hashes.insert(document.content_hash.clone()) {
                documents.push(document);
            }
        }

        documents.sort_by(|left, right| right.fetched_at.cmp(&left.fetched_at));
        Ok(documents)
    }

    async fn get_document(&self, id: &str) -> Result<Option<RawDocument>> {
        for path in self.all_document_paths()? {
            let document = Self::read_document(&path)?;
            if document.id.to_string() == id {
                return Ok(Some(document));
            }
        }

        Ok(None)
    }
}

#[derive(Debug)]
pub struct RuntimeStore {
    documents: DocumentArchive,
    extractions: RwLock<Vec<ExtractionResult>>,
}

impl RuntimeStore {
    pub fn new(document_root: impl Into<PathBuf>) -> Self {
        Self {
            documents: DocumentArchive::new(document_root),
            extractions: RwLock::new(Vec::new()),
        }
    }

    pub fn document_archive(&self) -> &DocumentArchive {
        &self.documents
    }
}

#[async_trait]
impl DocumentRepository for RuntimeStore {
    async fn save_document(&self, document: RawDocument) -> Result<()> {
        self.documents.save_document(document).await
    }

    async fn list_documents(&self) -> Result<Vec<RawDocument>> {
        self.documents.list_documents().await
    }

    async fn get_document(&self, id: &str) -> Result<Option<RawDocument>> {
        self.documents.get_document(id).await
    }
}

#[async_trait]
impl ExtractionRepository for RuntimeStore {
    async fn save_extraction(&self, extraction: ExtractionResult) -> Result<()> {
        self.extractions.write().await.push(extraction);
        Ok(())
    }

    async fn list_extractions(&self) -> Result<Vec<ExtractionResult>> {
        Ok(self.extractions.read().await.clone())
    }
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl DocumentRepository for InMemoryStore {
    async fn save_document(&self, document: RawDocument) -> Result<()> {
        let mut documents = self.documents.write().await;
        if documents
            .iter()
            .any(|existing| existing.content_hash == document.content_hash)
        {
            return Ok(());
        }

        documents.push(document);
        Ok(())
    }

    async fn list_documents(&self) -> Result<Vec<RawDocument>> {
        Ok(self.documents.read().await.clone())
    }

    async fn get_document(&self, id: &str) -> Result<Option<RawDocument>> {
        Ok(self
            .documents
            .read()
            .await
            .iter()
            .find(|document| document.id.to_string() == id)
            .cloned())
    }
}

#[async_trait]
impl ExtractionRepository for InMemoryStore {
    async fn save_extraction(&self, extraction: ExtractionResult) -> Result<()> {
        self.extractions.write().await.push(extraction);
        Ok(())
    }

    async fn list_extractions(&self) -> Result<Vec<ExtractionResult>> {
        Ok(self.extractions.read().await.clone())
    }
}

fn storage_error(error: std::io::Error) -> ObjectiveError {
    ObjectiveError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use objective_core::types::{BodyFormat, RawDocument};
    use std::collections::HashMap;
    use ulid::Ulid;

    use super::*;

    fn document(hash: &str) -> RawDocument {
        RawDocument {
            id: Ulid::new(),
            source_id: "fixture".to_string(),
            source_type: "rss".to_string(),
            external_id: hash.to_string(),
            url: None,
            title: Some("Fixture".to_string()),
            body: "A valid body with enough text for storage.".to_string(),
            body_format: BodyFormat::PlainText,
            author: None,
            published_at: None,
            fetched_at: chrono::Utc::now(),
            language: "en".to_string(),
            content_hash: hash.to_string(),
            metadata: HashMap::new(),
            raw_bytes: None,
        }
    }

    #[tokio::test]
    async fn test_save_document_deduplicates_by_content_hash() {
        let store = InMemoryStore::new();

        store.save_document(document("same")).await.unwrap();
        store.save_document(document("same")).await.unwrap();

        assert_eq!(store.list_documents().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_document_archive_persists_gzip_json_by_month_partition() {
        let tempdir = tempfile::tempdir().unwrap();
        let archive = DocumentArchive::new(tempdir.path());
        let document = document("archive-hash");
        let document_id = document.id.to_string();

        archive.save_document(document).await.unwrap();

        let saved = archive.get_document(&document_id).await.unwrap().unwrap();
        assert_eq!(saved.content_hash, "archive-hash");
        assert_eq!(archive.list_documents().await.unwrap().len(), 1);
        assert!(archive
            .all_document_paths()
            .unwrap()
            .iter()
            .any(|path| path.to_string_lossy().ends_with(".json.gz")));
    }

    #[tokio::test]
    async fn test_document_archive_deduplicates_by_hash() {
        let tempdir = tempfile::tempdir().unwrap();
        let archive = DocumentArchive::new(tempdir.path());

        archive.save_document(document("same")).await.unwrap();
        archive.save_document(document("same")).await.unwrap();

        assert_eq!(archive.list_documents().await.unwrap().len(), 1);
    }
}
