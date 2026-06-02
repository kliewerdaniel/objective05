use async_trait::async_trait;

use crate::{
    types::{ExtractionResult, RawDocument},
    Result,
};

#[async_trait]
pub trait DocumentRepository: Send + Sync {
    async fn save_document(&self, document: RawDocument) -> Result<()>;
    async fn list_documents(&self) -> Result<Vec<RawDocument>>;
    async fn get_document(&self, id: &str) -> Result<Option<RawDocument>>;
}

#[async_trait]
pub trait ExtractionRepository: Send + Sync {
    async fn save_extraction(&self, extraction: ExtractionResult) -> Result<()>;
    async fn list_extractions(&self) -> Result<Vec<ExtractionResult>>;
}
