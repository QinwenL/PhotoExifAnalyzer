use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use rayon::prelude::*;
use walkdir::WalkDir;

use super::ExifData;
use super::parser::parse_exif;

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jif", "jfif",
    "tiff", "tif",
    "png",
    "cr2", "cr3",
    "nef", "nrw",
    "arw", "srf", "sr2",
    "orf",
    "raf",
    "rw2",
    "pef",
    "dng",
    "raw", "rwl",
    "3fr",
    "kdc", "dcr",
    "mrw",
    "srw",
    "x3f",
    "bay",
];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanResult {
    pub path: PathBuf,
    pub exif: ExifData,
    pub file_size: u64,
    pub error: Option<String>,
}

pub struct ScanProgress {
    processed: usize,
    total: usize,
}

impl ScanProgress {
    fn new(total: usize) -> Self {
        ScanProgress { processed: 0, total }
    }

    fn increment(&mut self) {
        self.processed += 1;
    }

    fn percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.processed as f64 / self.total as f64) * 100.0
        }
    }
}

pub fn scan_directory<P: AsRef<Path>>(dir: P, recursive: bool) -> Vec<ScanResult> {
    scan_directory_with_callback(dir, recursive, |_| {}, || false)
}

pub fn scan_directory_with_callback<P: AsRef<Path>>(
    dir: P,
    recursive: bool,
    progress_callback: impl Fn(f64) + Send + Sync + 'static,
    cancel_check: impl Fn() -> bool + Send + Sync + 'static,
) -> Vec<ScanResult> {
    let dir = dir.as_ref();

    if !dir.exists() || !dir.is_dir() {
        return Vec::new();
    }

    let extensions: HashSet<&str> = IMAGE_EXTENSIONS.iter().copied().collect();

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

    let total_files = image_paths.len();
    let progress = Arc::new(RwLock::new(ScanProgress::new(total_files)));
    let results = Arc::new(Mutex::new(Vec::new()));

    image_paths.par_iter().for_each(|path| {
        if cancel_check() {
            return;
        }

        let result = process_image(path);
        results.lock().unwrap().push(result);

        let mut p = progress.write().unwrap();
        p.increment();
        progress_callback(p.percentage());
    });

    if cancel_check() {
        return Vec::new();
    }

    match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap(),
        Err(arc) => arc.lock().unwrap().clone(),
    }
}

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

        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        create_test_image(&sub_dir, "photo2.jpg");

        let results = scan_directory(temp_dir.path(), false);
        assert_eq!(results.len(), 1);

        let results = scan_directory(temp_dir.path(), true);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_scan_directory_nonexistent() {
        let results = scan_directory("/nonexistent/path", true);
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_with_progress_callback() {
        let temp_dir = TempDir::new().unwrap();
        create_test_image(temp_dir.path(), "photo1.jpg");
        create_test_image(temp_dir.path(), "photo2.jpg");
        create_test_image(temp_dir.path(), "photo3.jpg");

        let progress_values = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = Arc::clone(&progress_values);

        let results = scan_directory_with_callback(
            temp_dir.path(),
            true,
            move |p| progress_clone.lock().unwrap().push(p),
            || false,
        );

        assert_eq!(results.len(), 3);
        let values = progress_values.lock().unwrap();
        assert!(!values.is_empty());
        assert!(values.iter().any(|&v| v >= 100.0));
    }

    #[test]
    fn test_scan_with_cancel() {
        let temp_dir = TempDir::new().unwrap();
        create_test_image(temp_dir.path(), "photo1.jpg");
        create_test_image(temp_dir.path(), "photo2.jpg");
        create_test_image(temp_dir.path(), "photo3.jpg");

        let results = scan_directory_with_callback(
            temp_dir.path(),
            true,
            |_| {},
            || true,
        );

        assert!(results.is_empty());
    }
}
