//! Linux process-lineage, explicit-scope containment, and canary primitives.
//!
//! Containment is dry-run by default and only operates on process trees that
//! were explicitly registered by the caller. No host process is discovered and
//! terminated automatically.
pub mod canary;
pub mod containment;
pub mod lineage;

pub use canary::{Canary, CanaryManager};
pub use containment::{Containment, ContainmentAction, ContainmentReport};
pub use lineage::{Lineage, ProcessKey, ProcessNode};
