//! Title, description, and type inference for derived events.
//!
//! The first vertical slice uses a deterministic, heuristic
//! implementation that mirrors the structure the eventual LLM-backed
//! implementation will produce. The outputs are intentionally
//! short, factual, and free of editorial language so the
//! broadcasting layer can treat them as safe defaults.

use objective_core::types::{EventType, FirstClaim};

const TYPE_KEYWORDS: &[(&str, EventType)] = &[
    ("earnings", EventType::Business),
    ("revenue", EventType::Business),
    ("merger", EventType::Business),
    ("acquisition", EventType::Business),
    ("ipo", EventType::Business),
    ("stock", EventType::Business),
    ("shares", EventType::Business),
    ("ceo", EventType::Business),
    ("parliament", EventType::Politics),
    ("congress", EventType::Politics),
    ("senator", EventType::Politics),
    ("president", EventType::Politics),
    ("election", EventType::Politics),
    ("policy", EventType::Politics),
    ("legislation", EventType::Politics),
    ("chip", EventType::Technology),
    ("software", EventType::Technology),
    ("model", EventType::Technology),
    ("platform", EventType::Technology),
    ("startup", EventType::Technology),
    ("ai", EventType::Technology),
    ("study", EventType::Science),
    ("research", EventType::Science),
    ("discovery", EventType::Science),
    ("vaccine", EventType::Health),
    ("hospital", EventType::Health),
    ("disease", EventType::Health),
    ("outbreak", EventType::Health),
    ("pandemic", EventType::Health),
    ("treaty", EventType::World),
    ("diplomat", EventType::World),
    ("sanction", EventType::World),
    ("conflict", EventType::World),
    ("match", EventType::Sports),
    ("tournament", EventType::Sports),
    ("league", EventType::Sports),
    ("film", EventType::Entertainment),
    ("album", EventType::Entertainment),
    ("concert", EventType::Entertainment),
];

/// Heuristically classify a claim's text into an [`EventType`].
pub fn infer_event_type(text: &str) -> EventType {
    let lower = text.to_lowercase();
    for (keyword, event_type) in TYPE_KEYWORDS {
        if lower.contains(keyword) {
            return *event_type;
        }
    }
    EventType::Other
}

/// Build a concise, factual title for the first claim that produced
/// the event. The output is a noun-phrase based on the subject and
/// object of the claim, falling back to the first sentence of the
/// claim text when those fields are empty.
pub fn generate_event_title(claim: &FirstClaim) -> String {
    let subject = claim.subject_name.trim();
    let object = claim.object_name.as_deref().unwrap_or("").trim();

    if !subject.is_empty() && !object.is_empty() {
        return format!("{subject} and {object}");
    }
    if !subject.is_empty() {
        return first_meaningful_phrase(&claim.claim_text, subject);
    }
    first_sentence(&claim.claim_text).unwrap_or_else(|| "Unclassified event".to_string())
}

/// Build a longer description used as the event summary. The
/// description is a near-verbatim copy of the first claim that
/// produced the event so downstream consumers always have a citation
/// available.
pub fn generate_event_description(claim: &FirstClaim) -> String {
    first_sentence(&claim.claim_text).unwrap_or_else(|| claim.claim_text.trim().to_string())
}

fn first_sentence(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed
        .split(['.', '!', '?'])
        .map(str::trim)
        .find(|piece| !piece.is_empty())
        .map(|piece| {
            let mut sentence = piece.to_string();
            sentence.push('.');
            sentence
        })
}

fn first_meaningful_phrase(text: &str, subject: &str) -> String {
    if let Some(sentence) = first_sentence(text) {
        let prefix = format!("{subject} ");
        if sentence.len() > prefix.len() {
            return sentence;
        }
    }
    format!("{subject} update")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_claim(text: &str, subject: &str, object: Option<&str>) -> FirstClaim {
        FirstClaim {
            claim_id: "claim-1".to_string(),
            claim_text: text.to_string(),
            subject_name: subject.to_string(),
            object_name: object.map(|s| s.to_string()),
            location: None,
            published_at: Some(Utc::now()),
            confidence: 0.8,
            document_id: Some("doc-1".to_string()),
        }
    }

    #[test]
    fn test_infer_event_type_for_business_claim() {
        let claim = make_claim(
            "Apple reported record earnings in the latest quarter.",
            "Apple",
            None,
        );
        assert_eq!(infer_event_type(&claim.claim_text), EventType::Business);
    }

    #[test]
    fn test_infer_event_type_for_health_claim() {
        let claim = make_claim(
            "A new vaccine was approved by regulators.",
            "Regulators",
            None,
        );
        assert_eq!(infer_event_type(&claim.claim_text), EventType::Health);
    }

    #[test]
    fn test_generate_title_with_subject_and_object() {
        let claim = make_claim(
            "Apple Inc announced a new factory in Austin.",
            "Apple Inc",
            Some("Austin"),
        );
        assert_eq!(generate_event_title(&claim), "Apple Inc and Austin");
    }

    #[test]
    fn test_generate_title_falls_back_to_first_sentence() {
        let claim = make_claim("Markets rallied sharply today.", "", None);
        let title = generate_event_title(&claim);
        assert!(title.starts_with("Markets rallied sharply today"));
    }

    #[test]
    fn test_generate_description_returns_first_sentence() {
        let claim = make_claim(
            "Apple announced a new chip fab in Austin. Local officials confirmed the deal.",
            "Apple",
            None,
        );
        let description = generate_event_description(&claim);
        assert!(description.starts_with("Apple announced"));
        assert!(description.ends_with('.'));
    }
}
