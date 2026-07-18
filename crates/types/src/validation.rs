//! File validation logic for content ingestion

use crate::types::{ContentType, ValidationResult};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Validate a file and detect its content type
pub fn validate_file(path: &Path) -> ValidationResult {
    let mut result = ValidationResult::default();

    // Get file extension
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    // Detect content type from extension
    if let Some(content_type) = ContentType::from_extension(extension) {
        result.detected_type = Some(content_type);
    } else {
        result.warnings.push(format!(
            "Unknown file extension: {}. File may not be recognized.",
            extension
        ));
    }

    // Perform format-specific validation
    match result.detected_type {
        Some(ContentType::Map) => validate_pmtiles(path, &mut result),
        Some(ContentType::Book) => validate_book(path, &mut result),
        Some(ContentType::Poi) => validate_poi(path, &mut result),
        Some(ContentType::Model) => validate_model(path, &mut result),
        Some(ContentType::Misc) => validate_misc(path, &mut result),
        None => {
            result.warnings.push("Unable to determine file type. Content type validation skipped.".to_string());
        }
    }

    result
}

/// Validate PMTiles file format
fn validate_pmtiles(path: &Path, result: &mut ValidationResult) {
    // Check if file exists and is readable
    if !path.exists() {
        result.errors.push("File does not exist".to_string());
        result.valid = false;
        return;
    }

    // Try to read the file header
    match std::fs::read(path) {
        Ok(data) => {
            if data.len() < 7 {
                result.warnings.push("File is very small; may not be valid PMTiles".to_string());
            } else if &data[0..7] == b"PMTiles" {
                result.warnings.push("Valid PMTiles header detected".to_string());
            } else {
                result.warnings.push("File header does not match PMTiles format".to_string());
            }
        }
        Err(e) => {
            result.warnings.push(format!("Could not read file for validation: {}", e));
        }
    }
}

/// Validate book file format
fn validate_book(path: &Path, result: &mut ValidationResult) {
    if !path.exists() {
        result.errors.push("File does not exist".to_string());
        result.valid = false;
        return;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "epub" => {
            // EPUB is a ZIP file with specific structure
            result.warnings.push("EPUB validation: basic format check deferred to reader".to_string());
        }
        "pdf" | "mobi" => {
            result.warnings.push(format!("{} validation: deferred to reader", ext.to_uppercase()));
        }
        _ => {
            result.warnings.push("Unknown book format".to_string());
        }
    }
}

/// Validate POI file format
fn validate_poi(path: &Path, result: &mut ValidationResult) {
    if !path.exists() {
        result.errors.push("File does not exist".to_string());
        result.valid = false;
        return;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "fgb" => {
            result.warnings.push("FlatGeoBuf validation: deferred to renderer".to_string());
        }
        "geojson" | "json" => {
            result.warnings.push("GeoJSON validation: basic format check deferred".to_string());
        }
        _ => {
            result.warnings.push("Unknown POI format".to_string());
        }
    }
}

/// Validate GGUF model file format
fn validate_model(path: &Path, result: &mut ValidationResult) {
    if !path.exists() {
        result.errors.push("File does not exist".to_string());
        result.valid = false;
        return;
    }

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            result.errors.push(format!("Could not open model file: {}", e));
            result.valid = false;
            return;
        }
    };

    let mut magic = [0u8; 4];
    if let Err(e) = file.read_exact(&mut magic) {
        result
            .errors
            .push(format!("Could not read GGUF magic bytes: {}", e));
        result.valid = false;
        return;
    }

    if magic == *b"GGUF" {
        result
            .warnings
            .push("GGUF header detected; model file looks valid".to_string());
    } else {
        result
            .errors
            .push("Model file is not a valid GGUF archive (missing GGUF magic bytes)".to_string());
        result.valid = false;
    }
}

/// Validate generic files with a basic existence/readability check
fn validate_misc(path: &Path, result: &mut ValidationResult) {
    if !path.exists() {
        result.errors.push("File does not exist".to_string());
        result.valid = false;
        return;
    }

    if let Err(e) = std::fs::metadata(path) {
        result
            .warnings
            .push(format!("Could not inspect misc file metadata: {}", e));
    }
}
