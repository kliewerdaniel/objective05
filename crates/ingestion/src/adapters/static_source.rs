use std::time::Instant;

use async_trait::async_trait;
use objective_core::{
    traits::SourceAdapter,
    types::{BodyFormat, HealthStatus, PollResult, RateLimitConfig, RawDocument},
    Result,
};

use crate::normalizer::{DocumentInput, DocumentNormalizer};

#[derive(Debug, Clone)]
pub struct StaticSourceAdapter {
    name: String,
    documents: Vec<DocumentInput>,
    normalizer: DocumentNormalizer,
}

impl StaticSourceAdapter {
    pub fn new(name: impl Into<String>, documents: Vec<DocumentInput>) -> Self {
        Self {
            name: name.into(),
            documents,
            normalizer: DocumentNormalizer,
        }
    }

    pub fn from_plain_text(
        name: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        let name = name.into();
        Self::new(
            name.clone(),
            vec![DocumentInput {
                source_id: name,
                source_type: "fixture".to_string(),
                external_id: "fixture-1".to_string(),
                url: None,
                title: Some(title.into()),
                body: body.into(),
                body_format: BodyFormat::PlainText,
                author: None,
                published_at: None,
                metadata: std::collections::HashMap::new(),
                raw_bytes: None,
            }],
        )
    }
}

#[async_trait]
impl SourceAdapter for StaticSourceAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    fn source_type(&self) -> &str {
        "fixture"
    }

    fn validate(&self) -> Result<()> {
        Ok(())
    }

    async fn poll(&self, _cursor: Option<String>) -> Result<PollResult> {
        let started = Instant::now();
        let mut documents = Vec::with_capacity(self.documents.len());
        for input in self.documents.clone() {
            documents.push(self.normalizer.normalize(input)?);
        }

        Ok(PollResult {
            documents,
            new_cursor: Some(chrono::Utc::now().to_rfc3339()),
            has_more: false,
            poll_duration: started.elapsed(),
        })
    }

    async fn fetch_one(&self, external_id: &str) -> Result<RawDocument> {
        let input = self
            .documents
            .iter()
            .find(|document| document.external_id == external_id)
            .cloned()
            .ok_or_else(|| {
                objective_core::ObjectiveError::Source(format!("document not found: {external_id}"))
            })?;

        self.normalizer.normalize(input)
    }

    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            source_id: self.name.clone(),
            status: "healthy".to_string(),
            latency_ms: 0,
        })
    }

    fn rate_limit_config(&self) -> RateLimitConfig {
        RateLimitConfig {
            requests_per_minute: 60,
            burst: 5,
        }
    }
}
