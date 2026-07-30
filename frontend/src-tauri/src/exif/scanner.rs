use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use rayon::prelude::*;
use walkdir::WalkDir;

use super::ExifData;
use super::cache::ExifCache;
use super::parser::parse_exif;

/// Maximum number of concurrent I/O threads to avoid disk contention.
/// HDDs benefit from low concurrency (2-4); SSDs can handle more.
const MAX_IO_THREADS: usize = 4;

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
    scan_directory_with_cache(dir, recursive, None, progress_callback, cancel_check)
}

/// Phase 1 of two-phase scan: walk a directory and return image file paths
/// WITHOUT parsing EXIF. Used to populate the UI file list fast (<1s for
/// typical libraries) while EXIF parsing (phase 2) runs separately via
/// `scan_directory_with_cache`.
///
/// Reuses the same extension filter and `follow_links(false)` behavior as
/// the full scan so both phases see an identical file set.
pub fn scan_directory_quick<P: AsRef<Path>>(dir: P, recursive: bool) -> Vec<PathBuf> {
    let dir = dir.as_ref();
    if !dir.exists() || !dir.is_dir() {
        return Vec::new();
    }
    let extensions: HashSet<&str> = IMAGE_EXTENSIONS.iter().copied().collect();
    let walker = if recursive {
        WalkDir::new(dir).follow_links(false).into_iter()
    } else {
        WalkDir::new(dir).max_depth(1).follow_links(false).into_iter()
    };
    walker
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
        .collect()
}

/// Scan a directory for images with EXIF caching and I/O concurrency limiting.
///
/// # Arguments
/// * `dir` - Directory to scan
/// * `recursive` - Whether to scan subdirectories
/// * `cache` - Optional EXIF cache (wrapped in `Arc<Mutex<ExifCache>>` for thread safety)
/// * `progress_callback` - Called with progress percentage (0-100)
/// * `cancel_check` - Returns true if the scan should be cancelled
pub fn scan_directory_with_cache<P: AsRef<Path>>(
    dir: P,
    recursive: bool,
    cache: Option<Arc<Mutex<ExifCache>>>,
    progress_callback: impl Fn(f64) + Send + Sync + 'static,
    cancel_check: impl Fn() -> bool + Send + Sync + 'static,
) -> Vec<ScanResult> {
    let dir = dir.as_ref();

    // Phase 1: quick directory walk (no EXIF parsing) — shared with
    // `scan_directory_quick` so both phases see an identical file set.
    let image_paths = scan_directory_quick(dir, recursive);

    let total_files = image_paths.len();
    let progress = Arc::new(RwLock::new(ScanProgress::new(total_files)));
    let results = Arc::new(Mutex::new(Vec::new()));

    // Build a limited thread pool to avoid disk I/O contention.
    // When cache is populated, most lookups are fast SQLite reads;
    // for cache misses, this limits concurrent disk reads to MAX_IO_THREADS.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(MAX_IO_THREADS)
        .build()
        .expect("Failed to create I/O thread pool");

    let cache_clone = cache.clone();
    pool.install(|| {
        image_paths.par_iter().for_each(|path| {
            if cancel_check() {
                return;
            }

            let result = process_image_with_cache(path, cache_clone.as_ref());

            results.lock().unwrap().push(result);

            let mut p = progress.write().unwrap();
            p.increment();
            progress_callback(p.percentage());
        });
    });

    if cancel_check() {
        return Vec::new();
    }

    match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap(),
        Err(arc) => arc.lock().unwrap().clone(),
    }
}

/// Process a single image, using cache if available.
///
/// Cache hit: returns cached EXIF data without reading the file.
/// Cache miss: parses EXIF from disk, then stores result in cache.
fn process_image_with_cache(path: &Path, cache: Option<&Arc<Mutex<ExifCache>>>) -> ScanResult {
    let file_size = path
        .metadata()
        .map(|m| m.len())
        .unwrap_or(0);

    // 1. Check cache (fast SQLite lookup under mutex)
    if let Some(cache) = cache {
        if let Some(cached_exif) = cache.lock().unwrap().get(path) {
            return ScanResult {
                path: path.to_path_buf(),
                exif: cached_exif,
                file_size,
                error: None,
            };
        }
    }

    // 2. Cache miss — parse EXIF from disk
    let (exif, error) = match parse_exif(path) {
        Ok(exif) => (exif, None),
        Err(e) => (ExifData::new(), Some(e)),
    };

    // 3. Store successful parse results in cache
    if let Some(cache) = cache {
        if error.is_none() {
            let _ = cache.lock().unwrap().set(path, &exif);
        }
    }

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
    fn test_scan_with_cache_emits_progress() {
        // Task 4.10: progress callback must be invoked (not a no-op) on the cache path,
        // and values must monotonically increase to 100.
        let temp_dir = TempDir::new().unwrap();
        create_test_image(temp_dir.path(), "a.jpg");
        create_test_image(temp_dir.path(), "b.jpg");
        create_test_image(temp_dir.path(), "c.jpg");
        create_test_image(temp_dir.path(), "d.jpg");

        let cache = ExifCache::new(temp_dir.path()).unwrap();
        let cache = Arc::new(Mutex::new(cache));

        let progress_values = Arc::new(Mutex::new(Vec::new()));
        let progress_clone = Arc::clone(&progress_values);

        let results = scan_directory_with_cache(
            temp_dir.path(),
            true,
            Some(cache),
            move |p| progress_clone.lock().unwrap().push(p),
            || false,
        );

        assert_eq!(results.len(), 4);

        let values = progress_values.lock().unwrap();
        assert!(!values.is_empty(), "progress callback must be invoked on cache path");
        // Values should be monotonically non-decreasing
        for w in values.windows(2) {
            assert!(w[1] >= w[0], "progress must be non-decreasing: {:?}", *values);
        }
        // Final value must reach 100%
        assert_eq!(*values.last().unwrap(), 100.0, "final progress must be 100%");
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

    #[test]
    fn test_scan_with_cache_hit_returns_cached_data() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = create_test_image(temp_dir.path(), "cached.jpg");

        // Pre-populate cache with known EXIF data
        let cache = ExifCache::new(temp_dir.path()).unwrap();
        let test_exif = ExifData {
            make: Some("TestCamera".to_string()),
            model: Some("TestModel".to_string()),
            focal_length: Some(50.0),
            ..Default::default()
        };
        cache.set(&image_path, &test_exif).unwrap();

        // Scan with cache — should return cached EXIF without disk parsing
        let cache = Arc::new(Mutex::new(cache));
        let results = scan_directory_with_cache(
            temp_dir.path(),
            true,
            Some(cache),
            |_| {},
            || false,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].exif.make.as_deref(), Some("TestCamera"));
        assert_eq!(results[0].exif.model.as_deref(), Some("TestModel"));
        assert_eq!(results[0].exif.focal_length, Some(50.0));
        // Cache hit means no parse error
        assert!(results[0].error.is_none());
    }

    #[test]
    fn test_scan_without_cache_still_works() {
        let temp_dir = TempDir::new().unwrap();
        create_test_image(temp_dir.path(), "photo1.jpg");
        create_test_image(temp_dir.path(), "photo2.jpg");

        let results = scan_directory_with_cache(
            temp_dir.path(),
            true,
            None,
            |_| {},
            || false,
        );

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_scan_cache_miss_does_not_store_failed_parse() {
        let temp_dir = TempDir::new().unwrap();
        // Minimal JPEG without EXIF — parse_exif will return Err
        create_test_image(temp_dir.path(), "noexif.jpg");

        let cache = ExifCache::new(temp_dir.path()).unwrap();
        let cache = Arc::new(Mutex::new(cache));

        let results = scan_directory_with_cache(
            temp_dir.path(),
            true,
            Some(Arc::clone(&cache)),
            |_| {},
            || false,
        );

        assert_eq!(results.len(), 1);
        // Parse failed on minimal JPEG (no EXIF), so error should be set
        assert!(results[0].error.is_some());
        // Cache should NOT store failed parses
        let stats = cache.lock().unwrap().stats();
        assert_eq!(stats.total_entries, 0);
    }

    #[test]
    fn test_scan_with_cache_cancelled() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = create_test_image(temp_dir.path(), "photo.jpg");

        let cache = ExifCache::new(temp_dir.path()).unwrap();
        let test_exif = ExifData {
            make: Some("Canon".to_string()),
            ..Default::default()
        };
        cache.set(&image_path, &test_exif).unwrap();

        let cache = Arc::new(Mutex::new(cache));
        let results = scan_directory_with_cache(
            temp_dir.path(),
            true,
            Some(cache),
            |_| {},
            || true, // cancel immediately
        );

        assert!(results.is_empty());
    }

    // ---- Two-phase scan (task 4.7) ----

    #[test]
    fn test_quick_scan_returns_paths_without_parsing() {
        let temp_dir = TempDir::new().unwrap();
        create_test_image(temp_dir.path(), "a.jpg");
        create_test_image(temp_dir.path(), "b.jpg");
        std::fs::write(temp_dir.path().join("note.txt"), b"not an image").unwrap();

        let paths = scan_directory_quick(temp_dir.path(), true);
        assert_eq!(paths.len(), 2, "only image files should be returned");
        assert!(
            paths.iter().all(|p| p.extension().is_some()),
            "all returned paths must have an extension"
        );
    }

    #[test]
    fn test_quick_scan_respects_recursive_flag() {
        let temp_dir = TempDir::new().unwrap();
        create_test_image(temp_dir.path(), "top.jpg");
        let sub = temp_dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        create_test_image(&sub, "nested.jpg");

        let flat = scan_directory_quick(temp_dir.path(), false);
        assert_eq!(flat.len(), 1);

        let deep = scan_directory_quick(temp_dir.path(), true);
        assert_eq!(deep.len(), 2);
    }

    #[test]
    fn test_quick_scan_is_fast_under_one_second() {
        // Task 4.7 requirement: quick scan returns file list in <1s.
        // 100 files is enough to surface a regression without making the
        // test slow on CI.
        let temp_dir = TempDir::new().unwrap();
        for i in 0..100 {
            create_test_image(temp_dir.path(), &format!("img{i:03}.jpg"));
        }

        let start = std::time::Instant::now();
        let paths = scan_directory_quick(temp_dir.path(), true);
        let elapsed = start.elapsed();

        assert_eq!(paths.len(), 100);
        assert!(
            elapsed.as_millis() < 1000,
            "quick scan must be <1s, took {elapsed:?}"
        );
    }

    #[test]
    fn test_quick_scan_nonexistent_dir_returns_empty() {
        let paths = scan_directory_quick("/nonexistent/path", true);
        assert!(paths.is_empty());
    }
}
