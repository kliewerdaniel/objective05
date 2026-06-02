use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use objective_core::{
    traits::DocumentProcessor,
    types::{
        ClaimType, EntityType, ExtractedClaim, ExtractedEntity, ExtractedRelationship,
        ExtractionResult, RawDocument,
    },
    Result,
};
use regex::Regex;

#[derive(Debug, Default)]
pub struct HeuristicExtractionService;

#[async_trait]
impl DocumentProcessor for HeuristicExtractionService {
    async fn process(&self, document: &RawDocument) -> Result<ExtractionResult> {
        let entities = extract_entities(&document.body);
        let claims = extract_claims(&document.body, &entities);
        let relationships = extract_relationships(&document.body, &entities);

        Ok(ExtractionResult {
            document_id: document.id.to_string(),
            entities,
            claims,
            relationships,
        })
    }
}

fn extract_entities(body: &str) -> Vec<ExtractedEntity> {
    let entity_pattern = Regex::new(r"\b([A-Z][a-zA-Z0-9&.-]+(?:\s+[A-Z][a-zA-Z0-9&.-]+){0,3})\b")
        .expect("valid entity regex");
    let mut seen = HashSet::new();

    entity_pattern
        .captures_iter(body)
        .filter_map(|capture| {
            capture
                .get(1)
                .map(|match_| match_.as_str().trim().to_string())
        })
        .filter(|name| name.len() > 2 && !is_sentence_starter_noise(name))
        .filter(|name| seen.insert(name.clone()))
        .map(|name| {
            let entity_type = classify_entity(&name);
            ExtractedEntity {
                evidence_snippet: name.clone(),
                name,
                entity_type,
                aliases: Vec::new(),
                description: None,
                metadata: HashMap::new(),
                confidence: 0.55,
            }
        })
        .collect()
}

fn extract_claims(body: &str, entities: &[ExtractedEntity]) -> Vec<ExtractedClaim> {
    let sentences = body
        .split(['.', '!', '?'])
        .map(str::trim)
        .filter(|sentence| sentence.len() > 20);
    let entity_names = entities
        .iter()
        .map(|entity| entity.name.as_str())
        .collect::<Vec<_>>();

    sentences
        .filter_map(|sentence| {
            let subject = entity_names.iter().find(|name| sentence.contains(*name))?;
            Some(ExtractedClaim {
                claim_text: sentence.to_string(),
                subject_name: (*subject).to_string(),
                predicate: infer_predicate(sentence),
                object_name: entity_names
                    .iter()
                    .find(|name| **name != *subject && sentence.contains(*name))
                    .map(|name| (*name).to_string()),
                object_value: extract_numeric_value(sentence),
                claim_type: infer_claim_type(sentence),
                sentiment: None,
                confidence: 0.5,
                evidence_snippet: sentence.to_string(),
                attributed_to: None,
            })
        })
        .collect()
}

fn extract_relationships(body: &str, entities: &[ExtractedEntity]) -> Vec<ExtractedRelationship> {
    let names = entities
        .iter()
        .map(|entity| entity.name.as_str())
        .collect::<Vec<_>>();
    let mut relationships = Vec::new();

    for from in &names {
        for to in &names {
            if from == to {
                continue;
            }

            let located_pattern = format!("{from} in {to}");
            if body.contains(&located_pattern) {
                relationships.push(ExtractedRelationship {
                    from_entity_name: (*from).to_string(),
                    to_entity_name: (*to).to_string(),
                    relationship_type: "located_in".to_string(),
                    confidence: 0.55,
                    evidence_snippet: located_pattern,
                });
            }
        }
    }

    relationships
}

fn classify_entity(name: &str) -> EntityType {
    if name.ends_with("Inc")
        || name.ends_with("Corp")
        || name.ends_with("Company")
        || name.ends_with("LLC")
    {
        EntityType::Organization
    } else if matches!(
        name,
        "Austin" | "New York" | "Washington" | "London" | "United States"
    ) {
        EntityType::Location
    } else {
        EntityType::Concept
    }
}

fn infer_predicate(sentence: &str) -> String {
    let lower = sentence.to_lowercase();
    if lower.contains("announced") {
        "announced".to_string()
    } else if lower.contains("reported") {
        "reported".to_string()
    } else if lower.contains("said") || lower.contains("stated") {
        "stated".to_string()
    } else {
        "related_to".to_string()
    }
}

fn infer_claim_type(sentence: &str) -> ClaimType {
    if extract_numeric_value(sentence).is_some() {
        ClaimType::Quantification
    } else if sentence.to_lowercase().contains("said") || sentence.to_lowercase().contains("stated")
    {
        ClaimType::Attribution
    } else {
        ClaimType::Relation
    }
}

fn extract_numeric_value(sentence: &str) -> Option<String> {
    let numeric_pattern = Regex::new(r"\b\d+(?:\.\d+)?%?").expect("valid numeric regex");
    numeric_pattern
        .find(sentence)
        .map(|match_| match_.as_str().to_string())
}

fn is_sentence_starter_noise(name: &str) -> bool {
    matches!(name, "The" | "A" | "An" | "This" | "That")
}

#[cfg(test)]
mod tests {
    use objective_core::types::{BodyFormat, RawDocument};
    use std::collections::HashMap;
    use ulid::Ulid;

    use super::*;

    #[tokio::test]
    async fn test_process_extracts_entities_and_claims() {
        let document = RawDocument {
            id: Ulid::new(),
            source_id: "fixture".to_string(),
            source_type: "fixture".to_string(),
            external_id: "1".to_string(),
            url: None,
            title: Some("Fixture".to_string()),
            body: "Apple Inc announced a 10% manufacturing expansion in Austin. Analysts reported Apple Inc hired workers."
                .to_string(),
            body_format: BodyFormat::PlainText,
            author: None,
            published_at: None,
            fetched_at: chrono::Utc::now(),
            language: "en".to_string(),
            content_hash: "hash".to_string(),
            metadata: HashMap::new(),
            raw_bytes: None,
        };

        let result = HeuristicExtractionService.process(&document).await.unwrap();

        assert!(result
            .entities
            .iter()
            .any(|entity| entity.name == "Apple Inc"));
        assert!(result
            .claims
            .iter()
            .any(|claim| claim.object_value.as_deref() == Some("10%")));
    }
}
