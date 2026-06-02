pub mod adapters;
pub mod normalizer;
pub mod service;

pub use normalizer::{DocumentInput, DocumentNormalizer};
pub use service::IngestionService;
