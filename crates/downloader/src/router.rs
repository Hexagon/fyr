//! File routing logic — routes downloaded files to appropriate directories

use types::ContentType;
use std::path::{Path, PathBuf};

/// Routes files to the appropriate content directory based on type
pub struct ContentRouter;

impl ContentRouter {
    /// Determine destination directory for a file based on its extension
    pub fn route_file(file_path: &Path, data_dir: &Path) -> Option<PathBuf> {
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let content_type = ContentType::from_extension(extension)?;
        let destination_dir = data_dir.join(content_type.directory_name());

        // Preserve subdirectories if the file is in a subdirectory
        let file_name = file_path.file_name()?;
        let destination = destination_dir.join(file_name);

        Some(destination)
    }

    /// Check if a file type is recognized
    pub fn is_supported_type(file_path: &Path) -> bool {
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        ContentType::from_extension(extension).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_pmtiles() {
        let file = Path::new("test.pmtiles");
        let data_dir = Path::new("/data");
        let destination = ContentRouter::route_file(file, data_dir).unwrap();
        assert!(destination.to_string_lossy().contains("maps"));
    }

    #[test]
    fn test_route_epub() {
        let file = Path::new("book.epub");
        let data_dir = Path::new("/data");
        let destination = ContentRouter::route_file(file, data_dir).unwrap();
        assert!(destination.to_string_lossy().contains("books"));
    }

    #[test]
    fn test_route_fgb() {
        let file = Path::new("pois.fgb");
        let data_dir = Path::new("/data");
        let destination = ContentRouter::route_file(file, data_dir).unwrap();
        assert!(destination.to_string_lossy().contains("poi"));
    }
}
