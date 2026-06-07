//! In-memory stub [`KuzuGraphStore`] used when the `kuzu` feature is off.

use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use objective_core::{
    traits::GraphRepository,
    types::{ExtractedClaim, ExtractedEntity, ExtractedRelationship},
    Result,
};
use tracing::warn;

#[derive(Debug, Default)]
#[cfg_attr(feature = "kuzu", allow(dead_code))]
pub struct KuzuGraphStore {
    entities: RwLock<HashMap<String, ExtractedEntity>>,
    claims: RwLock<HashMap<String, ExtractedClaim>>,
    relationships: RwLock<Vec<ExtractedRelationship>>,
}

#[cfg_attr(feature = "kuzu", allow(dead_code))]
impl KuzuGraphStore {
    pub fn new<P: AsRef<std::path::Path>>(_path: P) -> Result<Self> {
        warn!(
            "KuzuGraphStore is the in-memory stub (compiled without the `kuzu` feature); \
             the path argument is ignored. Rebuild with `--features kuzu` to enable the \
             persistent Kuzu DB backend."
        );
        Ok(Self::default())
    }

    pub fn is_stub(&self) -> bool {
        true
    }
}

#[async_trait]
impl GraphRepository for KuzuGraphStore {
    async fn save_entity(&self, entity: ExtractedEntity) -> Result<()> {
        self.entities
            .write()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .insert(entity.name.clone(), entity);
        Ok(())
    }

    async fn get_entity(&self, name: &str) -> Result<Option<ExtractedEntity>> {
        Ok(self
            .entities
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .get(name)
            .cloned())
    }

    async fn list_entities(&self) -> Result<Vec<ExtractedEntity>> {
        Ok(self
            .entities
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .values()
            .cloned()
            .collect())
    }

    async fn save_claim(&self, claim: ExtractedClaim) -> Result<()> {
        self.claims
            .write()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .insert(claim.claim_text.clone(), claim);
        Ok(())
    }

    async fn get_claim(&self, claim_text: &str) -> Result<Option<ExtractedClaim>> {
        Ok(self
            .claims
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .get(claim_text)
            .cloned())
    }

    async fn list_claims(&self) -> Result<Vec<ExtractedClaim>> {
        Ok(self
            .claims
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .values()
            .cloned()
            .collect())
    }

    async fn save_relationship(&self, relationship: ExtractedRelationship) -> Result<()> {
        self.relationships
            .write()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .push(relationship);
        Ok(())
    }

    async fn get_relationship(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Option<ExtractedRelationship>> {
        Ok(self
            .relationships
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .iter()
            .find(|relationship| {
                relationship.from_entity_name == from && relationship.to_entity_name == to
            })
            .cloned())
    }

    async fn list_relationships(&self) -> Result<Vec<ExtractedRelationship>> {
        Ok(self
            .relationships
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .clone())
    }

    async fn find_related_entities(&self, entity_name: &str) -> Result<Vec<ExtractedEntity>> {
        let relationships = self
            .relationships
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .clone();
        let entities = self
            .entities
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .clone();
        let mut result = Vec::new();
        for relationship in &relationships {
            if relationship.from_entity_name == entity_name {
                if let Some(target) = entities.get(&relationship.to_entity_name) {
                    result.push(target.clone());
                }
            }
            if relationship.to_entity_name == entity_name {
                if let Some(target) = entities.get(&relationship.from_entity_name) {
                    result.push(target.clone());
                }
            }
        }
        Ok(result)
    }

    async fn find_claims_for_entity(&self, entity_name: &str) -> Result<Vec<ExtractedClaim>> {
        Ok(self
            .claims
            .read()
            .map_err(|error| objective_core::ObjectiveError::Storage(error.to_string()))?
            .values()
            .filter(|claim| claim.subject_name == entity_name)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_core::types::{ClaimType, EntityType};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_kuzu_stub_persists_entities_in_memory() {
        let tempdir = tempdir().unwrap();
        let store = KuzuGraphStore::new(tempdir.path()).unwrap();
        assert!(store.is_stub());

        let entity = ExtractedEntity {
            name: "Apple Inc".to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["Apple".to_string()],
            description: Some("Cupertino tech company".to_string()),
            metadata: std::collections::HashMap::new(),
            confidence: 0.9,
            evidence_snippet: "Apple Inc announced...".to_string(),
        };

        store.save_entity(entity.clone()).await.unwrap();
        let retrieved = store.get_entity("Apple Inc").await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Apple Inc");
        assert_eq!(retrieved.entity_type, EntityType::Organization);
        assert_eq!(store.list_entities().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_kuzu_stub_persists_claims() {
        let tempdir = tempdir().unwrap();
        let store = KuzuGraphStore::new(tempdir.path()).unwrap();

        let claim = ExtractedClaim {
            claim_text: "Apple expanded in Austin".to_string(),
            subject_name: "Apple Inc".to_string(),
            predicate: "expanded".to_string(),
            object_name: Some("Austin".to_string()),
            object_value: None,
            claim_type: ClaimType::Relation,
            sentiment: Some(0.5),
            confidence: 0.85,
            evidence_snippet: "Apple Inc announced expansion".to_string(),
            attributed_to: None,
        };

        store.save_claim(claim.clone()).await.unwrap();
        let retrieved = store
            .get_claim("Apple expanded in Austin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.confidence, 0.85);
        assert_eq!(store.list_claims().await.unwrap().len(), 1);
    }
}
