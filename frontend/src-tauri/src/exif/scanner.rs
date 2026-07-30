use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use walkdir::WalkDir;

use super::ExifData;
use super::parser::parse_exif;

/// Supported image file extensions
const IMAGE_EXTENSIONS: &[&str] = &[
    // JPEG
    "jpg", "jpeg", "jpe", "jif", "jfif",
    // TIFF
    "tiff", "tif",
    // PNG
    "png",
    // RAW formats
    "cr2", "cr3", // Canon
    "nef", "nrw", // Nikon
    "arw", "srf", "sr2", // Sony
    "orf", // Olympus
    "raf", // Fujifilm
    "rw2", // Panasonic
    "pef", // Pentax
    "dng", // Adobe DNG
    "raw", "rwl", // Leica
    "3fr", // Hasselblad
    "kdc", "dcr", // Kodak
    "mrw", // Minolta
    "srw", // Samsung
    "x3f", // Sigma
    "bay", // Casio
];

/// Scan result for a single image
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanResult {
    /// Path to the image file
    pub path: PathBuf,
    /// EXIF data (may be empty if parsing failed)
    pub exif: ExifData,
    /// File size in bytes
    pub file_size: u64,
    /// Error message if parsing failed
    pub error: Option<String>,
}

/// Scan a directory for images and extract EXIF data
///
/// # Arguments
/// * `dir` - Directory to scan
/// * `recursive` - Whether to scan subdirectories
///
/// # Returns
/// * `Vec<ScanResult>` - List of scan results
pub fn scan_directory<P: AsRef<Path>>(dir: P, recursive: bool) -> Vec<ScanResult> {
    let dir = dir.as_ref();

    if !dir.exists() || !dir.is_dir() {
        return Vec::new();
    }

    let extensions: HashSet<&str> = IMAGE_EXTENSIONS.iter().copied().collect();
    let results = Arc::new(Mutex::new(Vec::new()));

    let walker = if recursive {
        WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
    } else {
        WalkDir::new(dir)
            .max_depth(1)
            .follow_links(false)
            .into_iter()
    };

    // Collect all image paths first
    let image_paths: Vec<PathBuf> = walker
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| extensions.contains(ext.to_lowercase().as_str()))
                .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    // Process images in parallel
    image_paths.par_iter().for_each(|path| {
        let result = process_image(path);
        results.lock().unwrap().push(result);
    });

    match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap(),
        Err(arc) => arc.lock().unwrap().clone(),
    }
}

/// Process a single image file
fn process_image(path: &Path) -> ScanResult {
    let file_size = path
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0);

    let (exif, error) = match parse_exif(path) {
        Ok(exif) => (exif, None),
        Err(e) => (ExifData::new(), Some(e)),
    };

    ScanResult {
        path: path.to_path_buf(),
        exif,
        file_size,
        error,
    }
}

/// Check if a file extension is a supported image format
pub fn is_image_file<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref()
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_image(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        // Minimal valid JPEG
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xD9];
        let mut file = File::create(&path).unwrap();
        file.write_all(&jpeg).unwrap();
        path
    }

    #[test]
    fn test_is_image_file() {
        assert!(is_image_file("photo.jpg"));
        assert!(is_image_file("photo.JPEG"));
        assert!(is_image_file("photo.tiff"));
        assert!(is_image_file("photo.png"));
        assert!(is_image_file("photo.cr2"));
        assert!(is_image_file("photo.nef"));
        assert!(is_image_file("photo.arw"));
        assert!(!is_image_file("document.txt"));
        assert!(!is_image_file("video.mp4"));
    }

    #[test]
    fn test_scan_directory_empty() {
        let temp_dir = TempDir::new().unwrap();
        let results = scan_directory(temp_dir.path(), true);
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_directory_with_images() {
        let temp_dir = TempDir::new().unwrap();
        create_test_image(temp_dir.path(), "photo1.jpg");
        create_test_image(temp_dir.path(), "photo2.jpeg");
        create_test_image(temp_dir.path(), "document.txt");

        let results = scan_directory(temp_dir.path(), true);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_scan_directory_non_recursive() {
        let temp_dir = TempDir::new().unwrap();
        create_test_image(temp_dir.path(), "photo1.jpg");

        // Create subdirectory with image
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        create_test_image(&sub_dir, "photo2.jpg");

        // Non-recursive should only find 1 image
        let results = scan_directory(temp_dir.path(), false);
        assert_eq!(results.len(), 1);

        // Recursive should find both
        let results = scan_directory(temp_dir.path(), true);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_scan_directory_nonexistent() {
        let results = scan_directory("/nonexistent/path", true);
        assert!(results.is_empty());
    }
}
