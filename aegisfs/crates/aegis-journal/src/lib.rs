//! Experimental whole-file write-ahead journal for an isolated, trusted lab.
//!
//! This is NOT a FUSE filesystem or a production recovery mechanism. Only one
//! writer and one unresolved transaction are supported. External writers,
//! hostile paths, power failures and production durability are not certified.
mod filesystem;
mod journal;

pub use journal::{Journal, Pending, Phase};
