use std::path::Path;

use async_trait::async_trait;
use kuzu::{Connection, Database, SystemConfig};
use objective_core::{
    traits::GraphRepository,
    types::{ExtractedClaim, ExtractedEntity, ExtractedRelationship},
    ObjectiveError, Result,
};
use tracing::info;

pub struct KuzuGraphStore {
    _database: Database,
    connection: Connection,
}

impl KuzuGraphStore {
    pub fn new(path: &Path) -> Result<Self> {
        let config = SystemConfig::default();
        let database = Database::new(path, config).map_err(|error| {
            ObjectiveError::Storage(format!("failed to create Kuzu database: {error}"))
        })?;

        let connection = Connection::new(&database).map_err(|error| {
            ObjectiveError::Storage(format!("failed to create Kuzu connection: {error}"))
        })?;

        let store = Self {
            _database: database,
            connection,
        };

        store.initialize_schema()?;
        Ok(store)
    }

    fn initialize_schema(&self) -> Result<()> {
        info!("initializing Kuzu graph schema");

        self.connection
            .run("CREATE NODE TABLE IF NOT EXISTS Entity(name STRING PRIMARY KEY, entity_type STRING, aliases STRING, description STRING, confidence FLOAT, evidence_snippet STRING)")
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to create Entity table: {error}"))
            })?;

        self.connection
            .run("CREATE NODE TABLE IF NOT EXISTS Claim(claim_text STRING PRIMARY KEY, subject_name STRING, predicate STRING, object_name STRING, object_value STRING, claim_type STRING, sentiment FLOAT, confidence FLOAT, evidence_snippet STRING, attributed_to STRING)")
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to create Claim table: {error}"))
            })?;

        self.connection
            .run("CREATE REL TABLE IF NOT EXISTS RelatedTo(FROM Entity TO Entity, relationship_type STRING, confidence FLOAT, evidence_snippet STRING)")
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to create RelatedTo table: {error}"))
            })?;

        self.connection
            .run("CREATE REL TABLE IF NOT EXISTS Supports(FROM Entity TO Claim, confidence FLOAT)")
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to create Supports table: {error}"))
            })?;

        info!("Kuzu graph schema initialized");
        Ok(())
    }
}

#[async_trait]
impl GraphRepository for KuzuGraphStore {
    async fn save_entity(&self, entity: ExtractedEntity) -> Result<()> {
        let aliases_json = serde_json::to_string(&entity.aliases).map_err(|error| {
            ObjectiveError::Storage(format!("failed to serialize aliases: {error}"))
        })?;

        let query = format!(
            "CREATE (e:Entity {{name: '{}', entity_type: '{}', aliases: '{}', description: '{}', confidence: {}, evidence_snippet: '{}'}})",
            entity.name.replace('\'', "''"),
            format!("{:?}", entity.entity_type).replace('\'', "''"),
            aliases_json.replace('\'', "''"),
            entity.description.unwrap_or_default().replace('\'', "''"),
            entity.confidence,
            entity.evidence_snippet.replace('\'', "''")
        );

        self.connection.run(&query).map_err(|error| {
            ObjectiveError::Storage(format!("failed to save entity: {error}"))
        })?;

        Ok(())
    }

    async fn get_entity(&self, name: &str) -> Result<Option<ExtractedEntity>> {
        let query = format!(
            "MATCH (e:Entity {{name: '{}'}}) RETURN e.*",
            name.replace('\'', "''")
        );

        let result = self.connection.run(&query).map_err(|error| {
            ObjectiveError::Storage(format!("failed to get entity: {error}"))
        })?;

        if result.has_next() {
            let row = result.get_next().map_err(|error| {
                ObjectiveError::Storage(format!("failed to get entity row: {error}"))
            })?;

            let entity = parse_entity_row(&row)?;
            return Ok(Some(entity));
        }

        Ok(None)
    }

    async fn list_entities(&self) -> Result<Vec<ExtractedEntity>> {
        let result = self
            .connection
            .run("MATCH (e:Entity) RETURN e.*")
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to list entities: {error}"))
            })?;

        let mut entities = Vec::new();
        while result.has_next() {
            let row = result.get_next().map_err(|error| {
                ObjectiveError::Storage(format!("failed to get entity row: {error}"))
            })?;

            let entity = parse_entity_row(&row)?;
            entities.push(entity);
        }

        Ok(entities)
    }

    async fn save_claim(&self, claim: ExtractedClaim) -> Result<()> {
        let query = format!(
            "CREATE (c:Claim {{claim_text: '{}', subject_name: '{}', predicate: '{}', object_name: '{}', object_value: '{}', claim_type: '{}', sentiment: {}, confidence: {}, evidence_snippet: '{}', attributed_to: '{}'}})",
            claim.claim_text.replace('\'', "''"),
            claim.subject_name.replace('\'', "''"),
            claim.predicate.replace('\'', "''"),
            claim.object_name.unwrap_or_default().replace('\'', "''"),
            claim.object_value.unwrap_or_default().replace('\'', "''"),
            format!("{:?}", claim.claim_type).replace('\'', "''"),
            claim.sentiment.unwrap_or(0.0),
            claim.confidence,
            claim.evidence_snippet.replace('\'', "''"),
            claim.attributed_to.unwrap_or_default().replace('\'', "''")
        );

        self.connection.run(&query).map_err(|error| {
            ObjectiveError::Storage(format!("failed to save claim: {error}"))
        })?;

        Ok(())
    }

    async fn get_claim(&self, claim_text: &str) -> Result<Option<ExtractedClaim>> {
        let query = format!(
            "MATCH (c:Claim {{claim_text: '{}'}}) RETURN c.*",
            claim_text.replace('\'', "''")
        );

        let result = self.connection.run(&query).map_err(|error| {
            ObjectiveError::Storage(format!("failed to get claim: {error}"))
        })?;

        if result.has_next() {
            let row = result.get_next().map_err(|error| {
                ObjectiveError::Storage(format!("failed to get claim row: {error}"))
            })?;

            let claim = parse_claim_row(&row)?;
            return Ok(Some(claim));
        }

        Ok(None)
    }

    async fn list_claims(&self) -> Result<Vec<ExtractedClaim>> {
        let result = self
            .connection
            .run("MATCH (c:Claim) RETURN c.*")
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to list claims: {error}"))
            })?;

        let mut claims = Vec::new();
        while result.has_next() {
            let row = result.get_next().map_err(|error| {
                ObjectiveError::Storage(format!("failed to get claim row: {error}"))
            })?;

            let claim = parse_claim_row(&row)?;
            claims.push(claim);
        }

        Ok(claims)
    }

    async fn save_relationship(&self, relationship: ExtractedRelationship) -> Result<()> {
        let query = format!(
            "MATCH (a:Entity {{name: '{}'}}), (b:Entity {{name: '{}'}}) CREATE (a)-[:RelatedTo {{relationship_type: '{}', confidence: {}, evidence_snippet: '{}'}}]->(b)",
            relationship.from_entity_name.replace('\'', "''"),
            relationship.to_entity_name.replace('\'', "''"),
            relationship.relationship_type.replace('\'', "''"),
            relationship.confidence,
            relationship.evidence_snippet.replace('\'', "''")
        );

        self.connection.run(&query).map_err(|error| {
            ObjectiveError::Storage(format!("failed to save relationship: {error}"))
        })?;

        Ok(())
    }

    async fn get_relationship(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Option<ExtractedRelationship>> {
        let query = format!(
            "MATCH (a:Entity {{name: '{}'}})-[r:RelatedTo]->(b:Entity {{name: '{}'}}) RETURN r.*",
            from.replace('\'', "''"),
            to.replace('\'', "''")
        );

        let result = self.connection.run(&query).map_err(|error| {
            ObjectiveError::Storage(format!("failed to get relationship: {error}"))
        })?;

        if result.has_next() {
            let row = result.get_next().map_err(|error| {
                ObjectiveError::Storage(format!("failed to get relationship row: {error}"))
            })?;

            let relationship = parse_relationship_row(&row, from, to)?;
            return Ok(Some(relationship));
        }

        Ok(None)
    }

    async fn list_relationships(&self) -> Result<Vec<ExtractedRelationship>> {
        let result = self
            .connection
            .run("MATCH (a:Entity)-[r:RelatedTo]->(b:Entity) RETURN a.name, b.name, r.*")
            .map_err(|error| {
                ObjectiveError::Storage(format!("failed to list relationships: {error}"))
            })?;

        let mut relationships = Vec::new();
        while result.has_next() {
            let row = result.get_next().map_err(|error| {
                ObjectiveError::Storage(format!("failed to get relationship row: {error}"))
            })?;

            let from = row.get_value(0).as_str().unwrap_or("").to_string();
            let to = row.get_value(1).as_str().unwrap_or("").to_string();
            let relationship = parse_relationship_row(&row, &from, &to)?;
            relationships.push(relationship);
        }

        Ok(relationships)
    }

    async fn find_related_entities(&self, entity_name: &str) -> Result<Vec<ExtractedEntity>> {
        let query = format!(
            "MATCH (a:Entity {{name: '{}'}})-[:RelatedTo]->(b:Entity) RETURN b.*",
            entity_name.replace('\'', "''")
        );

        let result = self.connection.run(&query).map_err(|error| {
            ObjectiveError::Storage(format!("failed to find related entities: {error}"))
        })?;

        let mut entities = Vec::new();
        while result.has_next() {
            let row = result.get_next().map_err(|error| {
                ObjectiveError::Storage(format!("failed to get entity row: {error}"))
            })?;

            let entity = parse_entity_row(&row)?;
            entities.push(entity);
        }

        Ok(entities)
    }

    async fn find_claims_for_entity(&self, entity_name: &str) -> Result<Vec<ExtractedClaim>> {
        let query = format!(
            "MATCH (a:Entity {{name: '{}'}})-[:Supports]->(c:Claim) RETURN c.*",
            entity_name.replace('\'', "''")
        );

        let result = self.connection.run(&query).map_err(|error| {
            ObjectiveError::Storage(format!("failed to find claims for entity: {error}"))
        })?;

        let mut claims = Vec::new();
        while result.has_next() {
            let row = result.get_next().map_err(|error| {
                ObjectiveError::Storage(format!("failed to get claim row: {error}"))
            })?;

            let claim = parse_claim_row(&row)?;
            claims.push(claim);
        }

        Ok(claims)
    }
}

fn parse_entity_row(row: &kuzu::Value) -> Result<ExtractedEntity> {
    let name = row.get_value(0).as_str().unwrap_or("").to_string();
    let entity_type_str = row.get_value(1).as_str().unwrap_or("Person");
    let entity_type = match entity_type_str {
        "Person" => objective_core::types::EntityType::Person,
        "Organization" => objective_core::types::EntityType::Organization,
        "Location" => objective_core::types::EntityType::Location,
        "Concept" => objective_core::types::EntityType::Concept,
        "EventTopic" => objective_core::types::EntityType::EventTopic,
        _ => objective_core::types::EntityType::Person,
    };

    let aliases_str = row.get_value(2).as_str().unwrap_or("[]");
    let aliases: Vec<String> = serde_json::from_str(aliases_str).unwrap_or_default();

    let description = row
        .get_value(3)
        .as_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let confidence = row.get_value(4).as_float().unwrap_or(0.0) as f32;
    let evidence_snippet = row.get_value(5).as_str().unwrap_or("").to_string();

    Ok(ExtractedEntity {
        name,
        entity_type,
        aliases,
        description,
        metadata: std::collections::HashMap::new(),
        confidence,
        evidence_snippet,
    })
}

fn parse_claim_row(row: &kuzu::Value) -> Result<ExtractedClaim> {
    let claim_text = row.get_value(0).as_str().unwrap_or("").to_string();
    let subject_name = row.get_value(1).as_str().unwrap_or("").to_string();
    let predicate = row.get_value(2).as_str().unwrap_or("").to_string();
    let object_name = row
        .get_value(3)
        .as_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let object_value = row
        .get_value(4)
        .as_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let claim_type_str = row.get_value(5).as_str().unwrap_or("Attribution");
    let claim_type = match claim_type_str {
        "Attribution" => objective_core::types::ClaimType::Attribution,
        "Relation" => objective_core::types::ClaimType::Relation,
        "Quantification" => objective_core::types::ClaimType::Quantification,
        "Temporal" => objective_core::types::ClaimType::Temporal,
        "Comparison" => objective_core::types::ClaimType::Comparison,
        _ => objective_core::types::ClaimType::Attribution,
    };

    let sentiment = row.get_value(6).as_float().map(|v| v as f32);
    let confidence = row.get_value(7).as_float().unwrap_or(0.0) as f32;
    let evidence_snippet = row.get_value(8).as_str().unwrap_or("").to_string();
    let attributed_to = row
        .get_value(9)
        .as_str()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    Ok(ExtractedClaim {
        claim_text,
        subject_name,
        predicate,
        object_name,
        object_value,
        claim_type,
        sentiment,
        confidence,
        evidence_snippet,
        attributed_to,
    })
}

fn parse_relationship_row(
    row: &kuzu::Value,
    from: &str,
    to: &str,
) -> Result<ExtractedRelationship> {
    let relationship_type = row.get_value(2).as_str().unwrap_or("").to_string();
    let confidence = row.get_value(3).as_float().unwrap_or(0.0) as f32;
    let evidence_snippet = row.get_value(4).as_str().unwrap_or("").to_string();

    Ok(ExtractedRelationship {
        from_entity_name: from.to_string(),
        to_entity_name: to.to_string(),
        relationship_type,
        confidence,
        evidence_snippet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use objective_core::types::{ClaimType, EntityType};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_kuzu_graph_store_save_and_get_entity() {
        let tempdir = tempdir().unwrap();
        let store = KuzuGraphStore::new(tempdir.path()).unwrap();

        let entity = ExtractedEntity {
            name: "Test Entity".to_string(),
            entity_type: EntityType::Person,
            aliases: vec!["TE".to_string()],
            description: Some("A test entity".to_string()),
            metadata: std::collections::HashMap::new(),
            confidence: 0.9,
            evidence_snippet: "test evidence".to_string(),
        };

        store.save_entity(entity.clone()).await.unwrap();
        let retrieved = store.get_entity("Test Entity").await.unwrap().unwrap();

        assert_eq!(retrieved.name, "Test Entity");
        assert_eq!(retrieved.entity_type, EntityType::Person);
        assert_eq!(retrieved.confidence, 0.9);
    }

    #[tokio::test]
    async fn test_kuzu_graph_store_save_and_get_claim() {
        let tempdir = tempdir().unwrap();
        let store = KuzuGraphStore::new(tempdir.path()).unwrap();

        let claim = ExtractedClaim {
            claim_text: "Test claim".to_string(),
            subject_name: "Subject".to_string(),
            predicate: "is".to_string(),
            object_name: Some("Object".to_string()),
            object_value: None,
            claim_type: ClaimType::Attribution,
            sentiment: Some(0.5),
            confidence: 0.8,
            evidence_snippet: "test evidence".to_string(),
            attributed_to: None,
        };

        store.save_claim(claim.clone()).await.unwrap();
        let retrieved = store.get_claim("Test claim").await.unwrap().unwrap();

        assert_eq!(retrieved.claim_text, "Test claim");
        assert_eq!(retrieved.confidence, 0.8);
    }
}
