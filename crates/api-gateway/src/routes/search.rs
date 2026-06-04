use std::collections::BTreeSet;

use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::server::ApiState;

#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchQuery {
    /// Required search term. Case-insensitive substring match.
    pub q: String,
    /// Optional cap on the number of hits per kind. Defaults to 25.
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentHit {
    pub document_id: String,
    pub source_id: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EntityHit {
    pub name: String,
    pub entity_type: String,
    pub snippet: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ClaimHit {
    pub document_id: String,
    pub subject_name: String,
    pub predicate: String,
    pub object_name: Option<String>,
    pub claim_text: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResponse {
    pub query: String,
    pub document_hits: Vec<DocumentHit>,
    pub entity_hits: Vec<EntityHit>,
    pub claim_hits: Vec<ClaimHit>,
    pub total_hits: usize,
}

const DEFAULT_LIMIT: usize = 25;

fn snippet(body: &str, needle: &str) -> String {
    let lower = body.to_lowercase();
    if let Some(position) = lower.find(needle) {
        let start = position.saturating_sub(40);
        let end = (position + needle.len() + 80).min(body.len());
        let prefix = if start > 0 { "…" } else { "" };
        let suffix = if end < body.len() { "…" } else { "" };
        format!("{prefix}{}{suffix}", &body[start..end])
    } else {
        body.chars().take(160).collect::<String>() + "…"
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/search",
    params(SearchQuery),
    responses((status = 200, description = "Search across documents, entities, and claims", body = SearchResponse))
)]
pub async fn search(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> Json<SearchResponse> {
    let needle = query.q.trim().to_lowercase();
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).max(1);
    let documents = state.store.list_documents().await.unwrap_or_default();
    let extractions = state.store.list_extractions().await.unwrap_or_default();

    let mut document_hits: Vec<DocumentHit> = Vec::new();
    let mut entity_hits: Vec<EntityHit> = Vec::new();
    let mut claim_hits: Vec<ClaimHit> = Vec::new();

    for document in &documents {
        let title_hit = document
            .title
            .as_deref()
            .map(|t| t.to_lowercase().contains(&needle))
            .unwrap_or(false);
        let body_hit = document.body.to_lowercase().contains(&needle);
        if title_hit || body_hit {
            document_hits.push(DocumentHit {
                document_id: document.id.to_string(),
                source_id: document.source_id.clone(),
                title: document.title.clone(),
                url: document.url.clone(),
                snippet: snippet(&document.body, &needle),
            });
        }
    }

    let mut seen_entities: BTreeSet<String> = BTreeSet::new();
    for extraction in &extractions {
        for entity in &extraction.entities {
            if !seen_entities.contains(&entity.name) && entity.name.to_lowercase().contains(&needle)
            {
                seen_entities.insert(entity.name.clone());
                entity_hits.push(EntityHit {
                    name: entity.name.clone(),
                    entity_type: format!("{:?}", entity.entity_type),
                    snippet: snippet(&entity.evidence_snippet, &needle),
                });
            }
        }
        for claim in &extraction.claims {
            let blob = format!(
                "{} {} {} {}",
                claim.subject_name,
                claim.predicate,
                claim.object_name.as_deref().unwrap_or(""),
                claim.claim_text
            )
            .to_lowercase();
            if blob.contains(&needle) {
                claim_hits.push(ClaimHit {
                    document_id: extraction.document_id.clone(),
                    subject_name: claim.subject_name.clone(),
                    predicate: claim.predicate.clone(),
                    object_name: claim.object_name.clone(),
                    claim_text: claim.claim_text.clone(),
                });
            }
        }
    }

    document_hits.truncate(limit);
    entity_hits.truncate(limit);
    claim_hits.truncate(limit);

    let total_hits = document_hits.len() + entity_hits.len() + claim_hits.len();

    Json(SearchResponse {
        query: query.q,
        document_hits,
        entity_hits,
        claim_hits,
        total_hits,
    })
}
