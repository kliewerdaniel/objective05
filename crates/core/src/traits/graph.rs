use async_trait::async_trait;

use crate::{
    types::{ExtractedClaim, ExtractedEntity, ExtractedRelationship},
    Result,
};

#[async_trait]
pub trait GraphRepository: Send + Sync {
    async fn save_entity(&self, entity: ExtractedEntity) -> Result<()>;
    async fn get_entity(&self, name: &str) -> Result<Option<ExtractedEntity>>;
    async fn list_entities(&self) -> Result<Vec<ExtractedEntity>>;

    async fn save_claim(&self, claim: ExtractedClaim) -> Result<()>;
    async fn get_claim(&self, claim_text: &str) -> Result<Option<ExtractedClaim>>;
    async fn list_claims(&self) -> Result<Vec<ExtractedClaim>>;

    async fn save_relationship(&self, relationship: ExtractedRelationship) -> Result<()>;
    async fn get_relationship(&self, from: &str, to: &str) -> Result<Option<ExtractedRelationship>>;
    async fn list_relationships(&self) -> Result<Vec<ExtractedRelationship>>;

    async fn find_related_entities(&self, entity_name: &str) -> Result<Vec<ExtractedEntity>>;
    async fn find_claims_for_entity(&self, entity_name: &str) -> Result<Vec<ExtractedClaim>>;
}
