#[cfg(feature = "lancedb")]
mod lancedb;
#[cfg(not(feature = "lancedb"))]
mod stub;

#[cfg(feature = "lancedb")]
pub use lancedb::LanceVectorStore;
#[cfg(not(feature = "lancedb"))]
pub use stub::LanceVectorStore;
