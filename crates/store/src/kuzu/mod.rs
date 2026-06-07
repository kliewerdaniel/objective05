//! Kuzu-backed graph repository.
//!
//! The crate ships two implementations behind the same public type
//! `KuzuGraphStore`:
//!
//! - **Stub** (default): a thread-safe in-memory graph that preserves the
//!   [`GraphRepository`] contract. Always compiled, used by every default
//!   build, fast tests, and CI on machines without the C++ toolchain.
//! - **Real Kuzu DB** (feature `kuzu`): a persistent, ACID, Cypher-queryable
//!   graph store. Off by default because the C++ build chain (cmake + clang +
//!   libclang) is heavy on first compile. Build with:
//!   ```text
//!   cargo test -p objective-store --features kuzu
//!   ```
//!
//! `KuzuGraphStore` is a type alias for whichever impl is active. The
//! production wiring in `crates/objective/src/app.rs` does not need to know
//! which one is selected.

mod stub;

#[cfg(feature = "kuzu")]
mod real;

#[cfg(not(feature = "kuzu"))]
pub use stub::KuzuGraphStore;

#[cfg(feature = "kuzu")]
pub use real::KuzuGraphStore;

#[cfg(feature = "kuzu")]
pub use real::KuzuConfig;
