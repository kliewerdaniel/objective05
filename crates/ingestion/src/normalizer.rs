use std::collections::HashMap;

use chrono::{DateTime, Utc};
use objective_core::{
    types::{BodyFormat, RawDocument},
    ObjectiveError, Result,
};
use regex::Regex;
use sha2::{Digest, Sha256};
use ulid::Ulid;

const MIN_CONTENT_LENGTH: usize = 40;
const MAX_CONTENT_LENGTH: usize = 100_000;

#[derive(Debug, Clone)]
pub struct DocumentInput {
    pub source_id: String,
    pub source_type: String,
    pub external_id: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub body: String,
    pub body_format: BodyFormat,
    pub author: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub raw_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct DocumentNormalizer;

impl DocumentNormalizer {
    pub fn normalize(&self, input: DocumentInput) -> Result<RawDocument> {
        let body = normalize_text(&input.body, &input.body_format);
        if body.len() < MIN_CONTENT_LENGTH {
            return Err(ObjectiveError::Validation(format!(
                "document body is too short: {} characters",
                body.len()
            )));
        }

        let body = truncate_at_boundary(&body, MAX_CONTENT_LENGTH);
        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        Ok(RawDocument {
            id: Ulid::new(),
            source_id: input.source_id,
            source_type: input.source_type,
            external_id: input.external_id,
            url: input.url,
            title: input
                .title
                .map(|title| title.trim().to_string())
                .filter(|title| !title.is_empty()),
            body,
            body_format: BodyFormat::PlainText,
            author: input.author,
            published_at: input.published_at,
            fetched_at: Utc::now(),
            language: "en".to_string(),
            content_hash,
            metadata: input.metadata,
            raw_bytes: input.raw_bytes,
        })
    }
}

fn normalize_text(body: &str, body_format: &BodyFormat) -> String {
    let text = match body_format {
        BodyFormat::Html => strip_html(body),
        BodyFormat::Markdown | BodyFormat::PlainText => body.to_string(),
    };

    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_html(body: &str) -> String {
    let tags = Regex::new(r"<[^>]+>").expect("valid html tag regex");
    tags.replace_all(body, " ").to_string()
}

fn truncate_at_boundary(body: &str, max_len: usize) -> String {
    if body.len() <= max_len {
        return body.to_string();
    }

    body.char_indices()
        .take_while(|(index, _)| *index <= max_len)
        .map(|(_, character)| character)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_html_strips_tags_and_hashes_body() {
        let normalizer = DocumentNormalizer;
        let document = normalizer
            .normalize(DocumentInput {
                source_id: "feed".to_string(),
                source_type: "rss".to_string(),
                external_id: "1".to_string(),
                url: None,
                title: Some(" Example ".to_string()),
                body: "<p>Apple Inc announced a new manufacturing plan in Austin with public details.</p>"
                    .to_string(),
                body_format: BodyFormat::Html,
                author: None,
                published_at: None,
                metadata: HashMap::new(),
                raw_bytes: None,
            })
            .unwrap();

        assert_eq!(document.title.as_deref(), Some("Example"));
        assert!(!document.body.contains("<p>"));
        assert_eq!(document.content_hash.len(), 64);
    }

    #[test]
    fn test_normalize_rejects_tiny_documents() {
        let result = DocumentNormalizer.normalize(DocumentInput {
            source_id: "feed".to_string(),
            source_type: "rss".to_string(),
            external_id: "1".to_string(),
            url: None,
            title: None,
            body: "tiny".to_string(),
            body_format: BodyFormat::PlainText,
            author: None,
            published_at: None,
            metadata: HashMap::new(),
            raw_bytes: None,
        });

        assert!(result.is_err());
    }
}
