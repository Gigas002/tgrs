//! Telegram domain logic for tgrs: data model, in-memory store and the TDLib client.
//!
//! The library never parses CLI arguments, initializes logging, reads
//! configuration files or renders anything; callers pass resolved values in.

pub mod client;
pub mod model;
pub mod store;

/// The TDLib bindings, re-exported so callers use the same version.
pub use tdlib_rs;
