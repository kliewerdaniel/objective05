use async_trait::async_trait;

use crate::{
    types::{ExtractionResult, RawDocument},
    Result,
};

#[async_trait]
pub trait DocumentProcessor: Send + Sync {
    async fn process(&self, document: &RawDocument) -> Result<ExtractionResult>;
}
