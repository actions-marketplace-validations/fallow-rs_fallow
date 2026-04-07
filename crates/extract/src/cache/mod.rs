//! Incremental parse cache with bitcode serialization.
//!
//! Stores parsed module information (exports, imports, re-exports) on disk so
//! unchanged files can skip AST parsing on subsequent runs. Uses xxh3 content
//! hashing to detect changes.

mod conversion;
mod store;
mod types;

#[cfg(test)]
mod tests;

pub use conversion::{cached_to_module, module_to_cached};
pub use store::CacheStore;
pub use types::{
    CachedDynamicImport, CachedDynamicImportPattern, CachedExport, CachedImport, CachedMember,
    CachedModule, CachedReExport, CachedRequireCall, CachedSuppression,
};
