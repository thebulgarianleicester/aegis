//! Isolated-lab telemetry daemon for AegisFS.
//!
//! The server exposes the synthetic simulator and verified journal recovery.
//! It deliberately reports `protection_active=false`: no host filesystem or
//! host process is monitored or controlled by this build.
mod api;
mod engine;
pub mod http;
mod model;
mod websocket;

pub use engine::AppState;
pub use model::*;
pub use websocket::accept_key;
