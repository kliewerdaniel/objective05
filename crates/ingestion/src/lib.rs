pub mod adapters;
pub mod normalizer;
pub mod registry;
pub mod service;

pub use normalizer::{DocumentInput, DocumentNormalizer};
pub use registry::{RegistryError, SourceDefinition, SourcePatch, SourceRegistry, SourceType};
pub use service::IngestionService;
