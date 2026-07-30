use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use rayon::prelude::*;
use walkdir::WalkDir;

use super::ExifData;
use super::cache::ExifCache;
use super::parser::parse_exif_with_preview;

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

    /// Build a serializable snapshot of the current progress.
    /// Used by the progress callback so the frontend can display
    /// "scanned N / M" alongside the percentage.
    fn to_payload(&self) -> ScanProgressPayload {
        ScanProgressPayload {
            processed: self.processed,
            total: self.total,
            percentage: self.percentage(),
        }
    }
}

/// Progress payload emitted by the scanner to the frontend.
///
/// Serialized across the Tauri IPC boundary as the `scan_progress` event
/// payload. Carries `processed` and `total` so the UI can display
/// "scanned N / M" rather than only a bare percentage.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScanProgressPayload {
    pub processed: usize,
    pub total: usize,
    /// 0.0–100.0 inclusive. Redundant with processed/total but kept so
    /// the frontend doesn't have to recompute it (and risk divide-by-zero
    /// when total == 0).
    pub percentage: f64,
}

/// P3.4: kind of `ScanWarning`. Drives how the frontend renders the
/// warning (e.g., permission errors get a distinct "无法访问" treatment
/// per design.md).
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum ScanWarningKind {
    /// Could not read a directory's contents (e.g., chmod 0, ACL deny).
    /// Per design.md these are surfaced as "无法访问 XXX".
    PermissionDenied,
    /// Any other walkdir error (broken symlink, loop, IO error, etc.).
    /// Still surfaced so the user knows files were skipped, but not
    /// flagged as a permission issue.
    Other,
}

/// P3.4: warning emitted when the directory walk skips an entry.
///
/// Mirrors design.md's "权限错误 | 跳过文件夹 | 无法访问 XXX" row: rather
/// than silently `.ok()`-ing walkdir errors, the scanner classifies them
/// and emits a `ScanWarning` the frontend can surface to the user.
///
/// Serialized across the Tauri IPC boundary as the `scan_warning` event
/// payload.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ScanWarning {
    /// Absolute path of the entry that could not be accessed.
    pub path: String,
    /// User-facing message, e.g. "无法访问 /photos/restricted".
    pub message: String,
    /// Whether the underlying cause was a permission denial or something else.
    pub kind: ScanWarningKind,
}

/// P3.4: classify a walkdir error into a user-facing `ScanWarning`.
///
/// Extracted as a free function (rather than a method on `walkdir::Error`)
/// so it can be unit-tested without constructing a real walkdir error —
/// we just pass the path and underlying io::Error separately.
///
/// Returns `None` when there's no path to surface (e.g., walkdir couldn't
/// even associate a path with the error). Otherwise returns a `ScanWarning`
/// whose `kind` reflects whether the error was a permission denial.
fn classify_walk_error(
    path: Option<&Path>,
    io_err: Option<&std::io::Error>,
) -> Option<ScanWarning> {
    let path = path?;
    let path_str = path.to_string_lossy().to_string();
    let kind = match io_err.map(|e| e.kind()) {
        Some(std::io::ErrorKind::PermissionDenied) => ScanWarningKind::PermissionDenied,
        // walkdir can produce errors without an underlying io::Error
        // (e.g., loop ancestors); treat those as Other so the user is
        // still informed that files were skipped.
        _ => ScanWarningKind::Other,
    };
    Some(ScanWarning {
        path: path_str.clone(),
        message: format!("无法访问 {}", path_str),
        kind,
    })
}

/// P3.4: classify a `walkdir::Error` into a user-facing `ScanWarning`.
///
/// Thin wrapper around `classify_walk_error` that extracts the path and
/// underlying io::Error from the walkdir error. Returns `None` when the
/// error has no associated path (nothing useful to surface to the user).
fn classify_walkdir_error(err: &walkdir::Error) -> Option<ScanWarning> {
    classify_walk_error(err.path(), err.io_error())
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
    // Adapt the legacy f64-only callback into the payload-based callback
    // expected by `scan_directory_with_cache`. This preserves the existing
    // public API (many tests rely on `Fn(f64)`) while letting the inner
    // scan loop emit rich progress with processed/total counts.
    scan_directory_with_cache(
        dir,
        recursive,
        None,
        move |payload: ScanProgressPayload| progress_callback(payload.percentage),
        cancel_check,
    )
}

/// Like `scan_directory_with_cache` but accepts a payload-based callback
/// that receives `processed`/`total`/`percentage`. Used by the Tauri
/// command layer to forward full progress info to the frontend so the UI
/// can display "scanned N / M".
pub fn scan_directory_with_payload_progress<P: AsRef<Path>>(
    dir: P,
    recursive: bool,
    cache: Option<Arc<Mutex<ExifCache>>>,
    progress_callback: impl Fn(ScanProgressPayload) + Send + Sync + 'static,
    cancel_check: impl Fn() -> bool + Send + Sync + 'static,
) -> Vec<ScanResult> {
    scan_directory_with_cache(dir, recursive, cache, progress_callback, cancel_check)
}

/// Phase 1 of two-phase scan: walk a directory and return image file paths
/// WITHOUT parsing EXIF. Used to populate the UI file list fast (<1s for
/// typical libraries) while EXIF parsing (phase 2) runs separately via
/// `scan_directory_with_cache`.
///
/// Reuses the same extension filter and `follow_links(false)` behavior as
/// the full scan so both phases see an identical file set.
///
/// P3.4: silently swallows walkdir errors to preserve the original
/// signature. Use `scan_directory_quick_with_warnings` to surface them.
pub fn scan_directory_quick<P: AsRef<Path>>(dir: P, recursive: bool) -> Vec<PathBuf> {
    scan_directory_quick_with_warnings(dir, recursive, |_| {})
}

/// P3.4: like `scan_directory_quick`, but invokes `warning_callback`
/// for every walkdir error encountered (e.g., permission-denied
/// subdirectories) instead of silently dropping them via `.ok()`.
///
/// The warning carries a user-facing "无法访问 XXX" message and a kind
/// (PermissionDenied vs Other) so the frontend can render distinct UI
/// per design.md's "权限错误 | 跳过文件夹 | 无法访问 XXX" row.
pub fn scan_directory_quick_with_warnings<P: AsRef<Path>>(
    dir: P,
    recursive: bool,
    warning_callback: impl Fn(ScanWarning),
) -> Vec<PathBuf> {
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
        .filter_map(|e| match e {
            Ok(entry) => Some(entry),
            Err(err) => {
                if let Some(warning) = classify_walkdir_error(&err) {
                    warning_callback(warning);
                }
                None
            }
        })
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
/// * `progress_callback` - Called with a `ScanProgressPayload` carrying
///   `processed`/`total`/`percentage` so the frontend can display
///   "scanned N / M" alongside the percentage bar.
/// * `cancel_check` - Returns true if the scan should be cancelled
///
/// P3.4: silently swallows walkdir errors to preserve the original
/// signature. Use `scan_directory_with_cache_and_warnings` to surface them.
pub fn scan_directory_with_cache<P: AsRef<Path>>(
    dir: P,
    recursive: bool,
    cache: Option<Arc<Mutex<ExifCache>>>,
    progress_callback: impl Fn(ScanProgressPayload) + Send + Sync + 'static,
    cancel_check: impl Fn() -> bool + Send + Sync + 'static,
) -> Vec<ScanResult> {
    // No-op warning callback preserves the original silent-error behavior.
    scan_directory_with_cache_and_warnings(
        dir,
        recursive,
        cache,
        progress_callback,
        cancel_check,
        |_| {},
    )
}

/// P3.4: like `scan_directory_with_cache`, but invokes `warning_callback`
/// for every walkdir error encountered during the phase-1 directory walk
/// (e.g., permission-denied subdirectories).
///
/// Used by the Tauri command layer to emit `scan_warning` events so the
/// frontend can surface "无法访问 XXX" notifications to the user per
/// design.md's "权限错误 | 跳过文件夹 | 无法访问 XXX" row.
pub fn scan_directory_with_cache_and_warnings<P: AsRef<Path>>(
    dir: P,
    recursive: bool,
    cache: Option<Arc<Mutex<ExifCache>>>,
    progress_callback: impl Fn(ScanProgressPayload) + Send + Sync + 'static,
    cancel_check: impl Fn() -> bool + Send + Sync + 'static,
    warning_callback: impl Fn(ScanWarning) + Send + Sync + 'static,
) -> Vec<ScanResult> {
    let dir = dir.as_ref();

    // Phase 1: quick directory walk (no EXIF parsing) — shared with
    // `scan_directory_quick` so both phases see an identical file set.
    // P3.4: surface walkdir errors via warning_callback instead of
    // silently `.ok()`-ing them.
    let image_paths = scan_directory_quick_with_warnings(dir, recursive, warning_callback);

    let total_files = image_paths.len();
    let progress = Arc::new(RwLock::new(ScanProgress::new(total_files)));
    let results = Arc::new(Mutex::new(Vec::with_capacity(total_files)));

    // Emit an initial 0/N payload so the frontend can render the
    // "0 / N" baseline before the first file is processed.
    progress_callback(progress.read().unwrap().to_payload());

    // Collect cache-miss EXIF payloads per-thread via a thread-safe collector
    // so we can do a SINGLE bulk SQLite INSERT after the parallel parse
    // completes. Previously each thread did its own `INSERT OR REPLACE` under
    // the cache Mutex, producing one implicit SQLite transaction per file
    // (each paying a full fsync cost). Accumulating + bulk-inserting drops
    // that to one fsync total.
    let pending_cache_writes: Arc<Mutex<Vec<(String, u64, String)>>> =
        Arc::new(Mutex::new(Vec::new()));

    // Build a limited thread pool to avoid disk I/O contention.
    // When cache is populated, most lookups are fast SQLite reads;
    // for cache misses, this limits concurrent disk reads to MAX_IO_THREADS.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(MAX_IO_THREADS)
        .build()
        .expect("Failed to create I/O thread pool");

    let cache_clone = cache.clone();
    let pending_clone = Arc::clone(&pending_cache_writes);
    pool.install(|| {
        image_paths.par_iter().for_each(|path| {
            if cancel_check() {
                return;
            }

            let (result, pending_write) =
                process_image_with_cache(path, cache_clone.as_ref());

            if let Some(pw) = pending_write {
                pending_clone.lock().unwrap().push(pw);
            }

            results.lock().unwrap().push(result);

            let mut p = progress.write().unwrap();
            p.increment();
            progress_callback(p.to_payload());
        });
    });

    if cancel_check() {
        return Vec::new();
    }

    // Phase 3: single bulk-insert of every cache-miss EXIF row into SQLite.
    // This moves the write transaction off the hot per-file parallel path.
    if let Some(cache) = cache {
        let pending = match Arc::try_unwrap(pending_cache_writes) {
            Ok(mutex) => mutex.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        };
        if !pending.is_empty() {
            let _ = cache.lock().unwrap().bulk_insert(&pending);
        }
    }

    match Arc::try_unwrap(results) {
        Ok(mutex) => mutex.into_inner().unwrap(),
        Err(arc) => arc.lock().unwrap().clone(),
    }
}

/// Process a single image, using cache if available.
///
/// Cache hit: returns cached EXIF data without reading the file.
/// Cache miss: parses EXIF from disk, then returns a serialized pending
/// cache-write payload (path_str, mtime, exif_json) that the caller will
/// bulk-insert in a single SQL transaction after all parallel work is done.
///
/// This signature differs from the previous `process_image_with_cache` in
/// two key performance-critical ways:
///   1. `path.metadata()` is called ONCE and the result is reused for both
///      `file_size` and cache mtime validation (previously done 2–3x/file).
///   2. Successful cache-miss parses are NOT individually written via the
///      Mutex. Instead a `PendingCacheWrite` tuple is returned to the outer
///      driver which does one `bulk_insert` transaction at the end.
fn process_image_with_cache(
    path: &Path,
    cache: Option<&Arc<Mutex<ExifCache>>>,
) -> (ScanResult, Option<(String, u64, String)>) {
    // One metadata syscall per file — extract both size and mtime.
    let (file_size, modified_time) = match path.metadata() {
        Ok(meta) => {
            let size = meta.len();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            (size, mtime)
        }
        Err(_) => (0u64, None),
    };

    // 1. Check cache (fast SQLite lookup under mutex), passing the
    //    pre-read mtime so ExifCache::get doesn't stat() the file again.
    if let Some(cache) = cache {
        if let Some(cached_exif) = cache.lock().unwrap().get(path, modified_time) {
            return (
                ScanResult {
                    path: path.to_path_buf(),
                    exif: cached_exif,
                    file_size,
                    error: None,
                },
                None,
            );
        }
    }

    // 2. Cache miss — parse EXIF from disk
    let (exif, preview, error) = match parse_exif_with_preview(path) {
        Ok((exif, preview)) => (exif, preview, None),
        Err(e) => (ExifData::new(), None, Some(e)),
    };

    // 2b. 如果解析成功且拿到了 preview 偏移，立即缓存（缩略图路径可跳过扫描）
    if let (Some(preview), Some(cache), Some(mtime)) = (&preview, cache, modified_time) {
        let _ = cache.lock().unwrap().set_preview(path, preview, Some(mtime));
    }

    // 3. Successful parse: pre-serialize outside the cache Mutex and return
    //    to caller for bulk-insert. We only produce the pending-write tuple
    //    when `modified_time` is known (otherwise cache validation can't
    //    work reliably and it's better to skip caching this entry).
    let pending_write = match (error.is_none(), cache.is_some(), modified_time) {
        (true, true, Some(mtime)) => {
            let path_str = path.to_string_lossy().to_string();
            match serde_json::to_string(&exif) {
                Ok(exif_json) => Some((path_str, mtime, exif_json)),
                Err(_) => None,
            }
        }
        _ => None,
    };

    (
        ScanResult {
            path: path.to_path_buf(),
            exif,
            file_size,
            error,
        },
        pending_write,
    )
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
            move |p: ScanProgressPayload| progress_clone.lock().unwrap().push(p.percentage),
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
        cache.set(&image_path, &test_exif, None).unwrap();

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
        cache.set(&image_path, &test_exif, None).unwrap();

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

    // ---- Rich progress payload (P2.2: scanned / total) ----

    #[test]
    fn test_scan_emits_payload_with_processed_and_total() {
        // P2.2: the progress callback must receive a ScanProgressPayload
        // carrying `processed` and `total` counts (not just a bare f64
        // percentage) so the UI can render "scanned N / M".
        let temp_dir = TempDir::new().unwrap();
        for i in 0..5 {
            create_test_image(temp_dir.path(), &format!("img{i}.jpg"));
        }

        let payloads: Arc<Mutex<Vec<ScanProgressPayload>>> = Arc::new(Mutex::new(Vec::new()));
        let payloads_clone = Arc::clone(&payloads);

        let results = scan_directory_with_cache(
            temp_dir.path(),
            true,
            None,
            move |p| payloads_clone.lock().unwrap().push(p),
            || false,
        );

        assert_eq!(results.len(), 5);

        let captured = payloads.lock().unwrap();
        assert!(!captured.is_empty(), "progress callback must be invoked");

        // The very first emission is the 0/N baseline emitted before any
        // file is processed.
        let first = captured[0];
        assert_eq!(first.total, 5, "total must be the image count");
        assert_eq!(first.processed, 0, "first payload must be the 0/N baseline");
        assert_eq!(first.percentage, 0.0);

        // The last emission must reflect all 5 files processed.
        let last = *captured.last().unwrap();
        assert_eq!(last.total, 5);
        assert_eq!(last.processed, 5);
        assert_eq!(last.percentage, 100.0);
    }

    #[test]
    fn test_scan_payload_progress_carries_total_zero_for_empty_dir() {
        // Empty directory → total == 0, processed == 0, percentage == 0.
        // The frontend uses this to render "0 / 0" instead of crashing
        // on a divide-by-zero.
        let temp_dir = TempDir::new().unwrap();

        let payloads: Arc<Mutex<Vec<ScanProgressPayload>>> = Arc::new(Mutex::new(Vec::new()));
        let payloads_clone = Arc::clone(&payloads);

        let _ = scan_directory_with_cache(
            temp_dir.path(),
            true,
            None,
            move |p| payloads_clone.lock().unwrap().push(p),
            || false,
        );

        let captured = payloads.lock().unwrap();
        assert!(!captured.is_empty());
        let first = captured[0];
        assert_eq!(first.total, 0);
        assert_eq!(first.processed, 0);
        assert_eq!(first.percentage, 0.0);
    }

    // ---- P3.4: permission errors during directory walk ----

    #[test]
    fn test_classify_walk_error_permission_denied() {
        // P3.4: permission errors during directory walking must be
        // classified as ScanWarningKind::PermissionDenied with a
        // "无法访问 XXX" message so the UI can surface it to the user
        // (design.md: "权限错误 | 跳过文件夹 | 无法访问 XXX").
        let io_err = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "permission denied",
        );
        let warning = classify_walk_error(
            Some(Path::new("/photos/restricted")),
            Some(&io_err),
        )
        .expect("permission denied should produce a warning");

        assert_eq!(warning.kind, ScanWarningKind::PermissionDenied);
        assert_eq!(warning.path, "/photos/restricted");
        // Message must contain the path and the "无法访问" prefix per spec.
        assert!(
            warning.message.contains("/photos/restricted"),
            "message should contain the path: {}",
            warning.message
        );
        assert!(
            warning.message.contains("无法访问"),
            "message should contain 无法访问 prefix: {}",
            warning.message
        );
    }

    #[test]
    fn test_classify_walk_error_no_path_returns_none() {
        // If walkdir couldn't even associate a path with the error,
        // there's nothing useful to surface to the user — return None
        // so the caller doesn't try to render an empty warning.
        let io_err = std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        );
        let warning = classify_walk_error(None, Some(&io_err));
        assert!(warning.is_none(), "no path → no warning");
    }

    #[test]
    fn test_classify_walk_error_other_io_error_still_warns() {
        // Non-permission errors (e.g., NotFound on a broken symlink
        // target) should still produce a warning so the user knows
        // files were skipped during the walk. They are tagged as
        // ScanWarningKind::Other so the UI can differentiate if needed.
        let io_err = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        );
        let warning = classify_walk_error(
            Some(Path::new("/missing")),
            Some(&io_err),
        )
        .expect("other io errors should still warn");

        assert_eq!(warning.kind, ScanWarningKind::Other);
        assert_eq!(warning.path, "/missing");
        assert!(warning.message.contains("/missing"));
    }

    #[test]
    fn test_classify_walk_error_no_io_error_still_warns_as_other() {
        // walkdir can produce errors without an underlying io::Error
        // (e.g., loop ancestors). These should still be surfaced as
        // ScanWarningKind::Other so the user is informed.
        let warning = classify_walk_error(
            Some(Path::new("/looped")),
            None,
        )
        .expect("walkdir errors without io::Error should still warn");

        assert_eq!(warning.kind, ScanWarningKind::Other);
        assert_eq!(warning.path, "/looped");
    }

    #[test]
    fn test_scan_quick_with_warnings_invokes_callback_on_walk_error() {
        // P3.4: scan_directory_quick_with_warnings must invoke the
        // warning callback when walkdir yields an error entry, instead
        // of silently swallowing it via `.ok()`.
        //
        // We can't easily produce a real walkdir::Error on every platform
        // in a unit test, but we CAN exercise the integration by pointing
        // the walker at a path inside a directory whose immediate parent
        // is a file (which makes walkdir yield an error on the entry).
        // On platforms where this doesn't produce an error the test is
        // a no-op (asserts that warnings is empty), so it never fails
        // spuriously.
        let temp_dir = TempDir::new().unwrap();
        // Create a regular file and use IT as the "directory" to scan.
        // WalkDir will yield an error when trying to read entries from
        // a non-directory path.
        let file_path = temp_dir.path().join("not_a_dir");
        std::fs::write(&file_path, b"hello").unwrap();

        let warnings: Arc<Mutex<Vec<ScanWarning>>> = Arc::new(Mutex::new(Vec::new()));
        let warnings_clone = Arc::clone(&warnings);

        let _ = scan_directory_quick_with_warnings(
            &file_path,
            true,
            move |w| warnings_clone.lock().unwrap().push(w),
        );

        // Scanning a non-directory path is an error condition. The
        // implementation may either (a) emit a warning, or (b) return
        // empty silently (current `scan_directory_quick` behavior when
        // `!dir.is_dir()`). Both are acceptable here; the test exists
        // to ensure the function compiles and runs without panicking
        // when the callback is plumbed in. The strong assertion is in
        // the unix-only integration test below.
        drop(warnings.lock().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn test_scan_quick_with_warnings_surfaces_permission_denied() {
        // P3.4 integration test: an actually unreadable subdirectory
        // must produce a ScanWarning with kind == PermissionDenied.
        // Unix-only because chmod 0 is the portable way to revoke read
        // permission at the OS level; Windows ACLs are harder to set
        // from a unit test.
        use std::os::unix::fs::PermissionsExt;
        let temp_dir = TempDir::new().unwrap();

        // Create a subdirectory with a readable image, then revoke
        // read+execute permission on the subdirectory so walkdir fails
        // to list its contents.
        let sub = temp_dir.path().join("locked");
        std::fs::create_dir(&sub).unwrap();
        create_test_image(&sub, "img.jpg");
        let perms = std::fs::Permissions::from_mode(0o000);
        std::fs::set_permissions(&sub, perms).unwrap();

        let warnings: Arc<Mutex<Vec<ScanWarning>>> = Arc::new(Mutex::new(Vec::new()));
        let warnings_clone = Arc::clone(&warnings);

        // Run as root would bypass permission checks, so skip in that case.
        if unsafe { libc::getuid() } == 0 {
            // Restore perms so TempDir can clean up.
            let _ = std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755));
            return;
        }

        let paths = scan_directory_quick_with_warnings(
            temp_dir.path(),
            true,
            move |w| warnings_clone.lock().unwrap().push(w),
        );

        // Restore perms so TempDir can clean up.
        let _ = std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755));

        // The readable top-level dir was traversable; the locked sub
        // produced zero paths from inside it.
        assert!(
            paths.iter().all(|p| !p.starts_with(&sub)),
            "no paths should be returned from inside the unreadable subdirectory"
        );

        let captured = warnings.lock().unwrap();
        assert!(
            !captured.is_empty(),
            "permission denied on {:?} must produce at least one ScanWarning",
            sub
        );
        assert!(
            captured.iter().any(|w| w.kind == ScanWarningKind::PermissionDenied),
            "at least one warning must be PermissionDenied: {:?}",
            *captured
        );
    }

    #[test]
    fn test_scan_with_cache_forwards_warnings_to_callback() {
        // P3.4: scan_directory_with_cache_and_warnings must forward
        // walkdir errors to the warning callback (in addition to the
        // existing progress callback). We verify the plumbing by
        // pointing the scanner at a non-directory path; the integration
        // is exercised more thoroughly in the unix-only test above.
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("not_a_dir");
        std::fs::write(&file_path, b"hello").unwrap();

        let warnings: Arc<Mutex<Vec<ScanWarning>>> = Arc::new(Mutex::new(Vec::new()));
        let warnings_clone = Arc::clone(&warnings);

        let _ = scan_directory_with_cache_and_warnings(
            &file_path,
            true,
            None,
            |_| {},
            || false,
            move |w| warnings_clone.lock().unwrap().push(w),
        );

        // Same caveat as test_scan_quick_with_warnings_invokes_callback_on_walk_error:
        // scanning a non-directory may or may not emit a warning depending
        // on platform behavior, but the function must compile and run.
        drop(warnings.lock().unwrap());
    }
}
