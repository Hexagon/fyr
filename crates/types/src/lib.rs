//! Fyr Core — Shared types, data models, and configuration
//!
//! This crate contains:
//! - Data structures for content (maps, books, POIs)
//! - Download task management types
//! - Configuration management

pub mod types;
pub mod config;
pub mod settings;

pub use types::*;
pub use config::Config;
pub use settings::{AppSettings, GeoPosition};
