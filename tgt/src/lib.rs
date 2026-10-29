//! tgt binary internals, exposed as a library for integration tests.

pub mod app;
#[cfg(feature = "audio")]
pub mod audio;
pub mod cli;
pub mod config;
pub mod logger;
pub mod settings;
pub mod ui;
pub mod utils;
