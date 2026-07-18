use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoPosition {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AppSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<GeoPosition>,
    #[serde(default)]
    pub modules: HashMap<String, Value>,
}

impl AppSettings {
    pub fn with_location(location: Option<GeoPosition>) -> Self {
        Self {
            location,
            modules: HashMap::new(),
        }
    }
}