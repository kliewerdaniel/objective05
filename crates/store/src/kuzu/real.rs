//! Real Kuzu DB-backed [`KuzuGraphStore`], enabled by the `kuzu` feature.
//!
//! The store keeps a single `kuzu::Database` open for the lifetime of the
//! process (leaked into a `'static` reference because the cxx-generated
//! `Database` type is `!Send` and `!Sync`, while connections are `Send + Sync`
//! with a lifetime bound to the database). Every `GraphRepository` call opens
//! a fresh connection inside `tokio::task::spawn_blocking`, runs a single
//! Cypher statement, and returns the result. The kuzu API is fully
//! synchronous; the blocking pool keeps the async runtime responsive.
//!
//! Schema is migrated on first open with idempotent `CREATE ... IF NOT EXISTS`
//! statements:
//!
//! ```cypher
//! CREATE NODE TABLE IF NOT EXISTS Entity(
//!     name STRING,
//!     entity_type STRING,
//!     aliases STRING[],
//!     description STRING,
//!     confidence DOUBLE,
//!     evidence_snippet STRING,
//!     metadata_json STRING,
//!     PRIMARY KEY(name)
//! );
//! CREATE NODE TABLE IF NOT EXISTS Claim(
//!     claim_text STRING,
//!     subject_name STRING,
//!     predicate STRING,
//!     object_name STRING,
//!     object_value STRING,
//!     claim_type STRING,
//!     sentiment DOUBLE,
//!     confidence DOUBLE,
//!     evidence_snippet STRING,
//!     attributed_to STRING,
//!     PRIMARY KEY(claim_text)
//! );
//! CREATE REL TABLE IF NOT EXISTS Relationship(
//!     FROM Entity TO Entity,
//!     relationship_type STRING,
//!     confidence DOUBLE,
//!     evidence_snippet STRING
//! );
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use kuzu::{Database, LogicalType, SystemConfig, Value};
use objective_core::{
    traits::GraphRepository,
    types::{ClaimType, EntityType, ExtractedClaim, ExtractedEntity, ExtractedRelationship},
    Result,
};
use serde_json::Value as JsonValue;
use tracing::info;

/// Tuning knobs for the real Kuzu DB backend.
#[derive(Debug, Clone)]
pub struct KuzuConfig {
    /// Maximum number of threads the database may use for query execution.
    pub max_num_threads: u64,
    /// Per-connection query timeout in milliseconds. `0` disables the timeout.
    pub query_timeout_ms: u64,
}

impl Default for KuzuConfig {
    fn default() -> Self {
        Self {
            max_num_threads: 0,
            query_timeout_ms: 30_000,
        }
    }
}

/// Real Kuzu DB-backed implementation of [`GraphRepository`].
///
/// The struct owns an `Arc<DbHandle>` where `DbHandle` keeps the leaked
/// `Database` alive and the resolved `Path`. Cloning the `Arc` is cheap;
/// each `GraphRepository` call opens a fresh `kuzu::Connection` and runs the
/// query inside `tokio::task::spawn_blocking`.
pub struct KuzuGraphStore {
    inner: Arc<DbHandle>,
}

struct DbHandle {
    /// Leaked `Database` (lives for the process lifetime). `cxx` types are
    /// `!Send + !Sync`, so we leak once and share via the global `'static`
    /// borrow.
    db: &'static Database,
    /// On-disk path; only used for diagnostics.
    #[allow(dead_code)]
    path: PathBuf,
    /// Resolved config; stored for future telemetry and so callers can
    /// re-apply the per-connection timeout when a connection is recreated.
    #[allow(dead_code)]
    config: KuzuConfig,
}

impl std::fmt::Debug for KuzuGraphStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KuzuGraphStore")
            .field("path", &self.inner.path)
            .field("is_stub", &false)
            .finish()
    }
}

impl KuzuGraphStore {
    /// Open (or create) a Kuzu database at `path` and run schema migrations.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::with_config(path, KuzuConfig::default())
    }

    pub fn with_config<P: AsRef<Path>>(path: P, config: KuzuConfig) -> Result<Self> {
        let path = path.as_ref();
        // `path` is a *directory*; Kuzu expects a file path and creates
        // its database file at that location. Anchor the database file
        // inside the directory as `kuzu.db` so multiple stores can share
        // a parent (and so config keys like `graph_path` keep their
        // directory semantics from the stub era).
        std::fs::create_dir_all(path).map_err(|error| {
            objective_core::ObjectiveError::Storage(format!(
                "failed to create Kuzu database directory {}: {error}",
                path.display()
            ))
        })?;
        let db_path = path.join("kuzu.db");
        let system_config = SystemConfig::default();
        let db = Database::new(&db_path, system_config).map_err(map_kuzu_error)?;
        // SAFETY: `Database` is `!Send + !Sync` (cxx). The store is a
        // process-level singleton that lives for the daemon's lifetime, so
        // leaking the boxed database into a `'static` borrow is acceptable.
        // Connections (which are `Send + Sync` and lifetime-bound to the
        // database) can be created from any thread via `Connection::new(&db)`.
        let db_static: &'static Database = Box::leak(Box::new(db));

        // Apply the per-connection query timeout via a short-lived connection
        // (the timeout is stored on the connection, but we also keep it in
        // `DbHandle` so callers can re-apply it).
        if config.query_timeout_ms > 0 {
            if let Ok(conn) = kuzu::Connection::new(db_static) {
                conn.set_query_timeout(config.query_timeout_ms);
            }
        }

        let handle = DbHandle {
            db: db_static,
            path: db_path,
            config,
        };
        let store = Self {
            inner: Arc::new(handle),
        };
        store.migrate().map_err(|error| {
            objective_core::ObjectiveError::Storage(format!(
                "Kuzu schema migration failed: {error}"
            ))
        })?;
        info!(path = %path.display(), "KuzuGraphStore opened");
        Ok(store)
    }

    pub fn is_stub(&self) -> bool {
        false
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    fn migrate(&self) -> std::result::Result<(), String> {
        let db = self.inner.db;
        // Run synchronously on the calling thread — this only happens once
        // per process start, and Kuzu is fast at idempotent `IF NOT EXISTS`
        // statements.
        let conn = kuzu::Connection::new(db).map_err(|e| format!("{e}"))?;
        for stmt in SCHEMA_STATEMENTS {
            if let Err(error) = conn.query(stmt) {
                // CREATE ... IF NOT EXISTS is supposed to be idempotent, but
                // if Kuzu has not bootstrapped the database directory yet we
                // still need to surface the error.
                return Err(format!("`{stmt}` failed: {error}"));
            }
        }
        Ok(())
    }
}

const SCHEMA_STATEMENTS: &[&str] = &[
    "CREATE NODE TABLE IF NOT EXISTS Entity(\
         name STRING, \
         entity_type STRING, \
         aliases STRING[], \
         description STRING, \
         confidence DOUBLE, \
         evidence_snippet STRING, \
         metadata_json STRING, \
         PRIMARY KEY(name)\
     );",
    "CREATE NODE TABLE IF NOT EXISTS Claim(\
         claim_text STRING, \
         subject_name STRING, \
         predicate STRING, \
         object_name STRING, \
         object_value STRING, \
         claim_type STRING, \
         sentiment DOUBLE, \
         confidence DOUBLE, \
         evidence_snippet STRING, \
         attributed_to STRING, \
         PRIMARY KEY(claim_text)\
     );",
    "CREATE REL TABLE IF NOT EXISTS Relationship(\
         FROM Entity TO Entity, \
         relationship_type STRING, \
         confidence DOUBLE, \
         evidence_snippet STRING\
     );",
];

fn map_kuzu_error(error: kuzu::Error) -> objective_core::ObjectiveError {
    objective_core::ObjectiveError::Storage(format!("kuzu: {error}"))
}

fn map_storage_error(message: String) -> objective_core::ObjectiveError {
    objective_core::ObjectiveError::Storage(format!("kuzu: {message}"))
}

/// Run a closure that takes a `&kuzu::Connection` and returns a `Result<T, String>`
/// inside `tokio::task::spawn_blocking`. The closure is `Send + 'static`; the
/// leaked `'static` database makes the connection lifetime 'static too.
async fn run_blocking<F, T>(db: &'static Database, f: F) -> std::result::Result<T, String>
where
    F: FnOnce(&kuzu::Connection) -> std::result::Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let result = tokio::task::spawn_blocking(move || {
        let conn = kuzu::Connection::new(db).map_err(|error| format!("{error}"))?;
        f(&conn)
    })
    .await
    .map_err(|error| format!("join error: {error}"))?;
    result
}

#[async_trait]
impl GraphRepository for KuzuGraphStore {
    async fn save_entity(&self, entity: ExtractedEntity) -> Result<()> {
        let db = self.inner.db;
        let name = entity.name.clone();
        let entity_type = entity_type_to_string(&entity.entity_type).to_string();
        let aliases = entity.aliases.clone();
        let description = entity.description.clone();
        let confidence = entity.confidence as f64;
        let evidence_snippet = entity.evidence_snippet.clone();
        // Kuzu does not support storing `serde_json::Value` directly; encode
        // the metadata map as a JSON string and round-trip it on read.
        let metadata_json = serde_json::to_string(&entity.metadata).unwrap_or_else(|_| "{}".into());

        let stmt = "MERGE (e:Entity {name: $name}) \
                    SET e.entity_type = $entity_type, \
                        e.aliases = $aliases, \
                        e.description = $description, \
                        e.confidence = $confidence, \
                        e.evidence_snippet = $evidence_snippet, \
                        e.metadata_json = $metadata_json;";
        run_blocking(db, move |conn| {
            let mut prepared = conn
                .prepare(stmt)
                .map_err(|error| format!("prepare save_entity: {error}"))?;
            let aliases_value = string_list_value(&aliases);
            conn.execute(
                &mut prepared,
                vec![
                    ("name", Value::String(name)),
                    ("entity_type", Value::String(entity_type)),
                    ("aliases", aliases_value),
                    ("description", Value::String(description.unwrap_or_default())),
                    ("confidence", Value::Double(confidence)),
                    ("evidence_snippet", Value::String(evidence_snippet)),
                    ("metadata_json", Value::String(metadata_json)),
                ],
            )
            .map_err(|error| format!("execute save_entity: {error}"))?;
            Ok(())
        })
        .await
        .map_err(map_storage_error)?;
        Ok(())
    }

    async fn get_entity(&self, name: &str) -> Result<Option<ExtractedEntity>> {
        let db = self.inner.db;
        let name = name.to_string();
        run_blocking(db, move |conn| {
            let mut prepared = conn
                .prepare(
                    "MATCH (e:Entity {name: $name}) \
                     RETURN e.name, e.entity_type, e.aliases, e.description, \
                            e.confidence, e.evidence_snippet, e.metadata_json;",
                )
                .map_err(|error| format!("prepare get_entity: {error}"))?;
            let mut result = conn
                .execute(
                    &mut prepared,
                    vec![("name", Value::String(name.clone()))],
                )
                .map_err(|error| format!("execute get_entity: {error}"))?;
            let decoded = match result.next() {
                Some(row) => Some(
                    entity_from_row(&row).map_err(|e| format!("decode get_entity: {e}"))?,
                ),
                None => None,
            };
            Ok(decoded)
        })
        .await
        .map_err(map_storage_error)
    }

    async fn list_entities(&self) -> Result<Vec<ExtractedEntity>> {
        let db = self.inner.db;
        run_blocking(db, move |conn| {
            let mut result = conn
                .query(
                    "MATCH (e:Entity) \
                     RETURN e.name, e.entity_type, e.aliases, e.description, \
                            e.confidence, e.evidence_snippet, e.metadata_json \
                     ORDER BY e.name;",
                )
                .map_err(|error| format!("list_entities query: {error}"))?;
            let mut entities = Vec::new();
            for row in result.by_ref() {
                entities.push(entity_from_row(&row).map_err(|e| format!("decode: {e}"))?);
            }
            Ok(entities)
        })
        .await
        .map_err(map_storage_error)
    }

    async fn save_claim(&self, claim: ExtractedClaim) -> Result<()> {
        let db = self.inner.db;
        let claim_text = claim.claim_text.clone();
        let subject_name = claim.subject_name.clone();
        let predicate = claim.predicate.clone();
        let object_name = claim.object_name.clone();
        let object_value = claim.object_value.clone();
        let claim_type = claim_type_to_string(&claim.claim_type).to_string();
        let sentiment = claim.sentiment.map(|s| s as f64);
        let confidence = claim.confidence as f64;
        let evidence_snippet = claim.evidence_snippet.clone();
        let attributed_to = claim.attributed_to.clone();

        let stmt = "MERGE (c:Claim {claim_text: $claim_text}) \
                    SET c.subject_name = $subject_name, \
                        c.predicate = $predicate, \
                        c.object_name = $object_name, \
                        c.object_value = $object_value, \
                        c.claim_type = $claim_type, \
                        c.sentiment = $sentiment, \
                        c.confidence = $confidence, \
                        c.evidence_snippet = $evidence_snippet, \
                        c.attributed_to = $attributed_to;";
        run_blocking(db, move |conn| {
            let mut prepared = conn
                .prepare(stmt)
                .map_err(|error| format!("prepare save_claim: {error}"))?;
            conn.execute(
                &mut prepared,
                vec![
                    ("claim_text", Value::String(claim_text)),
                    ("subject_name", Value::String(subject_name)),
                    ("predicate", Value::String(predicate)),
                    ("object_name", Value::String(object_name.unwrap_or_default())),
                    ("object_value", Value::String(object_value.unwrap_or_default())),
                    ("claim_type", Value::String(claim_type)),
                    ("sentiment", optional_double_value(sentiment)),
                    ("confidence", Value::Double(confidence)),
                    ("evidence_snippet", Value::String(evidence_snippet)),
                    ("attributed_to", Value::String(attributed_to.unwrap_or_default())),
                ],
            )
            .map_err(|error| format!("execute save_claim: {error}"))?;
            Ok(())
        })
        .await
        .map_err(map_storage_error)?;
        Ok(())
    }

    async fn get_claim(&self, claim_text: &str) -> Result<Option<ExtractedClaim>> {
        let db = self.inner.db;
        let claim_text = claim_text.to_string();
        run_blocking(db, move |conn| {
            let mut prepared = conn
                .prepare(
                    "MATCH (c:Claim {claim_text: $claim_text}) \
                     RETURN c.claim_text, c.subject_name, c.predicate, c.object_name, \
                            c.object_value, c.claim_type, c.sentiment, c.confidence, \
                            c.evidence_snippet, c.attributed_to;",
                )
                .map_err(|error| format!("prepare get_claim: {error}"))?;
            let mut result = conn
                .execute(
                    &mut prepared,
                    vec![("claim_text", Value::String(claim_text.clone()))],
                )
                .map_err(|error| format!("execute get_claim: {error}"))?;
            let decoded = match result.next() {
                Some(row) => Some(
                    claim_from_row(&row).map_err(|e| format!("decode get_claim: {e}"))?,
                ),
                None => None,
            };
            Ok(decoded)
        })
        .await
        .map_err(map_storage_error)
    }

    async fn list_claims(&self) -> Result<Vec<ExtractedClaim>> {
        let db = self.inner.db;
        run_blocking(db, move |conn| {
            let mut result = conn
                .query(
                    "MATCH (c:Claim) \
                     RETURN c.claim_text, c.subject_name, c.predicate, c.object_name, \
                            c.object_value, c.claim_type, c.sentiment, c.confidence, \
                            c.evidence_snippet, c.attributed_to \
                     ORDER BY c.claim_text;",
                )
                .map_err(|error| format!("list_claims query: {error}"))?;
            let mut claims = Vec::new();
            for row in result.by_ref() {
                claims.push(claim_from_row(&row).map_err(|e| format!("decode: {e}"))?);
            }
            Ok(claims)
        })
        .await
        .map_err(map_storage_error)
    }

    async fn save_relationship(&self, relationship: ExtractedRelationship) -> Result<()> {
        let db = self.inner.db;
        let from_entity_name = relationship.from_entity_name.clone();
        let to_entity_name = relationship.to_entity_name.clone();
        let relationship_type = relationship.relationship_type.clone();
        let confidence = relationship.confidence as f64;
        let evidence_snippet = relationship.evidence_snippet.clone();

        let stmt = "MATCH (a:Entity {name: $from}), (b:Entity {name: $to}) \
                    MERGE (a)-[r:Relationship {relationship_type: $relationship_type}]->(b) \
                    SET r.confidence = $confidence, \
                        r.evidence_snippet = $evidence_snippet;";
        run_blocking(db, move |conn| {
            let mut prepared = conn
                .prepare(stmt)
                .map_err(|error| format!("prepare save_relationship: {error}"))?;
            conn.execute(
                &mut prepared,
                vec![
                    ("from", Value::String(from_entity_name)),
                    ("to", Value::String(to_entity_name)),
                    ("relationship_type", Value::String(relationship_type)),
                    ("confidence", Value::Double(confidence)),
                    ("evidence_snippet", Value::String(evidence_snippet)),
                ],
            )
            .map_err(|error| format!("execute save_relationship: {error}"))?;
            Ok(())
        })
        .await
        .map_err(map_storage_error)?;
        Ok(())
    }

    async fn get_relationship(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Option<ExtractedRelationship>> {
        let db = self.inner.db;
        let from = from.to_string();
        let to = to.to_string();
        run_blocking(db, move |conn| {
            let mut prepared = conn
                .prepare(
                    "MATCH (a:Entity {name: $from})-[r:Relationship]->(b:Entity {name: $to}) \
                     RETURN r.relationship_type, r.confidence, r.evidence_snippet \
                     LIMIT 1;",
                )
                .map_err(|error| format!("prepare get_relationship: {error}"))?;
            let mut result = conn
                .execute(
                    &mut prepared,
                    vec![
                        ("from", Value::String(from)),
                        ("to", Value::String(to)),
                    ],
                )
                .map_err(|error| format!("execute get_relationship: {error}"))?;
            let decoded = match result.next() {
                Some(row) => Some(
                    relationship_from_row(&row)
                        .map_err(|e| format!("decode get_relationship: {e}"))?,
                ),
                None => None,
            };
            Ok(decoded)
        })
        .await
        .map_err(map_storage_error)
    }

    async fn list_relationships(&self) -> Result<Vec<ExtractedRelationship>> {
        let db = self.inner.db;
        run_blocking(db, move |conn| {
            let mut result = conn
                .query(
                    "MATCH (a:Entity)-[r:Relationship]->(b:Entity) \
                     RETURN a.name AS from, b.name AS to, r.relationship_type, \
                            r.confidence, r.evidence_snippet \
                     ORDER BY a.name, b.name, r.relationship_type;",
                )
                .map_err(|error| format!("list_relationships query: {error}"))?;
            let mut relationships = Vec::new();
            for row in result.by_ref() {
                relationships.push(
                    relationship_from_named_row(&row).map_err(|e| format!("decode: {e}"))?,
                );
            }
            Ok(relationships)
        })
        .await
        .map_err(map_storage_error)
    }

    async fn find_related_entities(&self, entity_name: &str) -> Result<Vec<ExtractedEntity>> {
        let db = self.inner.db;
        let entity_name = entity_name.to_string();
        run_blocking(db, move |conn| {
            let mut prepared = conn
                .prepare(
                    "MATCH (a:Entity {name: $name})-[r:Relationship]-(b:Entity) \
                     RETURN DISTINCT b.name, b.entity_type, b.aliases, b.description, \
                                     b.confidence, b.evidence_snippet, b.metadata_json;",
                )
                .map_err(|error| format!("prepare find_related_entities: {error}"))?;
            let mut result = conn
                .execute(
                    &mut prepared,
                    vec![("name", Value::String(entity_name))],
                )
                .map_err(|error| format!("execute find_related_entities: {error}"))?;
            let mut entities = Vec::new();
            for row in result.by_ref() {
                entities.push(entity_from_row(&row).map_err(|e| format!("decode: {e}"))?);
            }
            Ok(entities)
        })
        .await
        .map_err(map_storage_error)
    }

    async fn find_claims_for_entity(&self, entity_name: &str) -> Result<Vec<ExtractedClaim>> {
        let db = self.inner.db;
        let entity_name = entity_name.to_string();
        run_blocking(db, move |conn| {
            let mut prepared = conn
                .prepare(
                    "MATCH (c:Claim) WHERE c.subject_name = $name \
                     RETURN c.claim_text, c.subject_name, c.predicate, c.object_name, \
                            c.object_value, c.claim_type, c.sentiment, c.confidence, \
                            c.evidence_snippet, c.attributed_to;",
                )
                .map_err(|error| format!("prepare find_claims_for_entity: {error}"))?;
            let mut result = conn
                .execute(
                    &mut prepared,
                    vec![("name", Value::String(entity_name))],
                )
                .map_err(|error| format!("execute find_claims_for_entity: {error}"))?;
            let mut claims = Vec::new();
            for row in result.by_ref() {
                claims.push(claim_from_row(&row).map_err(|e| format!("decode: {e}"))?);
            }
            Ok(claims)
        })
        .await
        .map_err(map_storage_error)
    }
}

// --- conversion helpers ---------------------------------------------------

fn entity_type_to_string(entity_type: &EntityType) -> &'static str {
    match entity_type {
        EntityType::Person => "person",
        EntityType::Organization => "organization",
        EntityType::Location => "location",
        EntityType::Concept => "concept",
        EntityType::EventTopic => "event_topic",
    }
}

fn string_to_entity_type(label: &str) -> std::result::Result<EntityType, String> {
    match label {
        "person" => Ok(EntityType::Person),
        "organization" => Ok(EntityType::Organization),
        "location" => Ok(EntityType::Location),
        "concept" => Ok(EntityType::Concept),
        "event_topic" => Ok(EntityType::EventTopic),
        other => Err(format!("unknown entity_type label: {other}")),
    }
}

fn claim_type_to_string(claim_type: &ClaimType) -> &'static str {
    match claim_type {
        ClaimType::Attribution => "attribution",
        ClaimType::Relation => "relation",
        ClaimType::Quantification => "quantification",
        ClaimType::Temporal => "temporal",
        ClaimType::Comparison => "comparison",
    }
}

fn string_to_claim_type(label: &str) -> std::result::Result<ClaimType, String> {
    match label {
        "attribution" => Ok(ClaimType::Attribution),
        "relation" => Ok(ClaimType::Relation),
        "quantification" => Ok(ClaimType::Quantification),
        "temporal" => Ok(ClaimType::Temporal),
        "comparison" => Ok(ClaimType::Comparison),
        other => Err(format!("unknown claim_type label: {other}")),
    }
}

fn string_value(v: &Value) -> std::result::Result<String, String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Null(_) => Ok(String::new()),
        other => Err(format!("expected string, got {other:?}")),
    }
}

fn double_value(v: &Value) -> std::result::Result<f64, String> {
    match v {
        Value::Double(d) => Ok(*d),
        Value::Float(f) => Ok(*f as f64),
        Value::Int64(n) => Ok(*n as f64),
        Value::Null(_) => Ok(0.0),
        other => Err(format!("expected double, got {other:?}")),
    }
}

fn optional_double_value(value: Option<f64>) -> Value {
    match value {
        Some(d) => Value::Double(d),
        None => Value::Null(LogicalType::Double),
    }
}

fn string_list_value(values: &[String]) -> Value {
    let elements: Vec<Value> = values.iter().cloned().map(Value::String).collect();
    Value::List(LogicalType::String, elements)
}

fn read_string_list(v: &Value) -> std::result::Result<Vec<String>, String> {
    match v {
        Value::List(_, items) => items.iter().map(string_value).collect(),
        Value::Null(_) => Ok(Vec::new()),
        other => Err(format!("expected list, got {other:?}")),
    }
}

fn optional_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn metadata_from_json(json: &str) -> HashMap<String, JsonValue> {
    serde_json::from_str(json).unwrap_or_default()
}

fn entity_from_row(row: &[Value]) -> std::result::Result<ExtractedEntity, String> {
    let name = string_value(&row[0])?;
    let entity_type = string_to_entity_type(&string_value(&row[1])?)?;
    let aliases = read_string_list(&row[2])?;
    let description = optional_string(string_value(&row[3])?);
    let confidence = double_value(&row[4])? as f32;
    let evidence_snippet = string_value(&row[5])?;
    let metadata = metadata_from_json(&string_value(&row[6])?);
    Ok(ExtractedEntity {
        name,
        entity_type,
        aliases,
        description,
        metadata,
        confidence,
        evidence_snippet,
    })
}

fn claim_from_row(row: &[Value]) -> std::result::Result<ExtractedClaim, String> {
    let claim_text = string_value(&row[0])?;
    let subject_name = string_value(&row[1])?;
    let predicate = string_value(&row[2])?;
    let object_name = optional_string(string_value(&row[3])?);
    let object_value = optional_string(string_value(&row[4])?);
    let claim_type = string_to_claim_type(&string_value(&row[5])?)?;
    let sentiment = match &row[6] {
        Value::Null(_) => None,
        other => Some(double_value(other)? as f32),
    };
    let confidence = double_value(&row[7])? as f32;
    let evidence_snippet = string_value(&row[8])?;
    let attributed_to = optional_string(string_value(&row[9])?);
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

fn relationship_from_row(row: &[Value]) -> std::result::Result<ExtractedRelationship, String> {
    let relationship_type = string_value(&row[0])?;
    let confidence = double_value(&row[1])? as f32;
    let evidence_snippet = string_value(&row[2])?;
    Ok(ExtractedRelationship {
        // from/to are not returned by the get_relationship query; the
        // caller already knows them, but we still need a valid struct, so
        // we set them to empty strings here. Callers that need the from/to
        // should use list_relationships or query the edge directly.
        from_entity_name: String::new(),
        to_entity_name: String::new(),
        relationship_type,
        confidence,
        evidence_snippet,
    })
}

fn relationship_from_named_row(
    row: &[Value],
) -> std::result::Result<ExtractedRelationship, String> {
    let from_entity_name = string_value(&row[0])?;
    let to_entity_name = string_value(&row[1])?;
    let relationship_type = string_value(&row[2])?;
    let confidence = double_value(&row[3])? as f32;
    let evidence_snippet = string_value(&row[4])?;
    Ok(ExtractedRelationship {
        from_entity_name,
        to_entity_name,
        relationship_type,
        confidence,
        evidence_snippet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_entity(name: &str) -> ExtractedEntity {
        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), JsonValue::String("test".to_string()));
        ExtractedEntity {
            name: name.to_string(),
            entity_type: EntityType::Organization,
            aliases: vec!["alias-a".to_string(), "alias-b".to_string()],
            description: Some(format!("{name} description")),
            metadata,
            confidence: 0.9,
            evidence_snippet: format!("{name} evidence"),
        }
    }

    fn sample_claim(text: &str, subject: &str) -> ExtractedClaim {
        ExtractedClaim {
            claim_text: text.to_string(),
            subject_name: subject.to_string(),
            predicate: "expanded".to_string(),
            object_name: Some("Austin".to_string()),
            object_value: None,
            claim_type: ClaimType::Relation,
            sentiment: Some(0.42),
            confidence: 0.85,
            evidence_snippet: format!("{text} evidence"),
            attributed_to: None,
        }
    }

    #[tokio::test]
    async fn real_kuzu_round_trip_for_entities() {
        let dir = tempdir().unwrap();
        let store = KuzuGraphStore::new(dir.path()).expect("open");
        assert!(!store.is_stub());

        let entity = sample_entity("Apple Inc");
        store.save_entity(entity.clone()).await.unwrap();
        let retrieved = store.get_entity("Apple Inc").await.unwrap().unwrap();
        assert_eq!(retrieved.name, "Apple Inc");
        assert_eq!(retrieved.entity_type, EntityType::Organization);
        assert_eq!(retrieved.aliases, entity.aliases);
        assert_eq!(retrieved.confidence, 0.9);
        assert_eq!(retrieved.description, entity.description);
        assert_eq!(
            retrieved.metadata.get("source").and_then(|v| v.as_str()),
            Some("test"),
        );
        assert_eq!(store.list_entities().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn real_kuzu_round_trip_for_claims() {
        let dir = tempdir().unwrap();
        let store = KuzuGraphStore::new(dir.path()).expect("open");
        let claim = sample_claim("Apple expanded in Austin", "Apple Inc");
        store.save_claim(claim.clone()).await.unwrap();
        let retrieved = store
            .get_claim("Apple expanded in Austin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.subject_name, "Apple Inc");
        assert_eq!(retrieved.claim_type, ClaimType::Relation);
        assert_eq!(retrieved.sentiment, Some(0.42));
        assert_eq!(retrieved.confidence, 0.85);
        assert_eq!(store.list_claims().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn real_kuzu_relationships_traversal() {
        let dir = tempdir().unwrap();
        let store = KuzuGraphStore::new(dir.path()).expect("open");
        store.save_entity(sample_entity("Apple Inc")).await.unwrap();
        store.save_entity(sample_entity("Cupertino")).await.unwrap();
        let rel = ExtractedRelationship {
            from_entity_name: "Apple Inc".to_string(),
            to_entity_name: "Cupertino".to_string(),
            relationship_type: "headquartered_in".to_string(),
            confidence: 0.95,
            evidence_snippet: "Apple is headquartered in Cupertino".to_string(),
        };
        store.save_relationship(rel.clone()).await.unwrap();
        let listed = store.list_relationships().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].from_entity_name, "Apple Inc");
        assert_eq!(listed[0].to_entity_name, "Cupertino");
        assert_eq!(listed[0].relationship_type, "headquartered_in");

        let related = store.find_related_entities("Apple Inc").await.unwrap();
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].name, "Cupertino");

        let related_back = store.find_related_entities("Cupertino").await.unwrap();
        assert_eq!(related_back.len(), 1);
        assert_eq!(related_back[0].name, "Apple Inc");
    }

    #[tokio::test]
    async fn real_kuzu_find_claims_for_entity() {
        let dir = tempdir().unwrap();
        let store = KuzuGraphStore::new(dir.path()).expect("open");
        store
            .save_claim(sample_claim("Apple expanded in Austin", "Apple Inc"))
            .await
            .unwrap();
        store
            .save_claim(sample_claim("Apple acquired Beats", "Apple Inc"))
            .await
            .unwrap();
        store
            .save_claim(sample_claim("Google acquired Fitbit", "Alphabet"))
            .await
            .unwrap();
        let apple_claims = store.find_claims_for_entity("Apple Inc").await.unwrap();
        assert_eq!(apple_claims.len(), 2);
        let google_claims = store.find_claims_for_entity("Alphabet").await.unwrap();
        assert_eq!(google_claims.len(), 1);
    }

    #[tokio::test]
    async fn real_kuzu_persists_across_reopens() {
        let dir = tempdir().unwrap();
        {
            let store = KuzuGraphStore::new(dir.path()).expect("open");
            store.save_entity(sample_entity("Apple Inc")).await.unwrap();
            store.save_entity(sample_entity("Cupertino")).await.unwrap();
        }
        {
            let store = KuzuGraphStore::new(dir.path()).expect("reopen");
            let entities = store.list_entities().await.unwrap();
            assert_eq!(entities.len(), 2);
        }
    }

    #[tokio::test]
    async fn real_kuzu_get_missing_entity_returns_none() {
        let dir = tempdir().unwrap();
        let store = KuzuGraphStore::new(dir.path()).expect("open");
        let retrieved = store.get_entity("Does Not Exist").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn real_kuzu_get_missing_relationship_returns_none() {
        let dir = tempdir().unwrap();
        let store = KuzuGraphStore::new(dir.path()).expect("open");
        let retrieved = store
            .get_relationship("A", "B")
            .await
            .unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn real_kuzu_get_entity_matches_stub_contract() {
        let dir = tempdir().unwrap();
        let store = KuzuGraphStore::new(dir.path()).expect("open");
        // Save an entity, then look it up by name (the trait's keying
        // convention); this validates the same call shape as the stub.
        store
            .save_entity(sample_entity("OpenAI"))
            .await
            .unwrap();
        let retrieved = store.get_entity("OpenAI").await.unwrap().unwrap();
        assert_eq!(retrieved.name, "OpenAI");
        assert_eq!(retrieved.entity_type, EntityType::Organization);
    }

    #[test]
    fn kuzu_config_defaults() {
        let cfg = KuzuConfig::default();
        assert_eq!(cfg.query_timeout_ms, 30_000);
        assert_eq!(cfg.max_num_threads, 0);
    }
}
