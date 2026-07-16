//! Nexus Core — Shared types, data models, and configuration
//!
//! This crate contains:
//! - Data structures for content (maps, books, POIs)
//! - Download task management types
//! - Configuration management
//! - File validation logic

pub mod types;
pub mod config;
pub mod validation;

pub use types::*;
pub use config::Config;
