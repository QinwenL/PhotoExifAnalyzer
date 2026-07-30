pub mod exif;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use exif::cache::ExifCache;
use exif::scanner::{scan_directory_with_cache, ScanProgressPayload, ScanResult};
use exif::stats::{
    calculate_all_stats, calculate_camera_stats, calculate_focal_length_stats,
    calculate_lens_stats, filter_results, AllStats, CameraStats, FilterCriteria,
    FocalLengthStats, LensStats,
};
use serde::Serialize;
use exif::file_ops::delete_file;
use exif::thumbnail::{delete_thumbnail, delete_all_size_caches};

lazy_static::lazy_static! {
    static ref SCAN_CANCELLED: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    /// Global EXIF cache stored in the app's data directory.
    /// Initialized lazily on first scan; shared across all scan operations.
    /// None if initialization failed (see `CACHE_INIT_ERROR` for the reason).
    pub(crate) static ref EXIF_CACHE: Option<Arc<Mutex<ExifCache>>> = {
        let (cache, error) = init_cache_with_reason();
        if let Some(err) = &error {
            // P1.7: surface the failure reason instead of silently degrading.
            // The frontend can also query `get_cache_status` to show a banner.
            eprintln!("[warn] EXIF cache disabled: {}", err);
        }
        // Stash the error reason so `get_cache_status` can report it.
        if let Some(err) = error {
            *CACHE_INIT_ERROR.lock().unwrap() = Some(err);
        }
        cache
    };

    /// P1.7: Records WHY the EXIF cache failed to initialize (None if it
    /// succeeded or hasn't been queried yet). Surfaced to the frontend via
    /// the `get_cache_status` Tauri command so the UI can warn the user
    /// that repeat scans will be slower.
    static ref CACHE_INIT_ERROR: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
}

/// Initialize the EXIF cache in the user's app data directory.
/// Returns None if the directory cannot be determined or cache creation fails.
fn init_cache() -> Option<Arc<Mutex<ExifCache>>> {
    init_cache_with_reason().0
}

/// Like `init_cache` but also returns the failure reason (if any) so the
/// caller can log it or surface it to the UI. Used by the `EXIF_CACHE`
/// lazy_static initializer.
fn init_cache_with_reason() -> (Option<Arc<Mutex<ExifCache>>>, Option<String>) {
    let cache_dir = match get_cache_dir() {
        Some(d) => d,
        None => {
            return (
                None,
                Some(
                    "Could not determine app data directory (no APPDATA or HOME env var)"
                        .to_string(),
                ),
            )
        }
    };
    // Ensure the cache directory exists
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        return (
            None,
            Some(format!(
                "Failed to create cache directory at {}: {}",
                cache_dir.display(),
                e
            )),
        );
    }
    match ExifCache::new(&cache_dir) {
        Ok(cache) => (Some(Arc::new(Mutex::new(cache))), None),
        Err(e) => (
            None,
            Some(format!(
                "Failed to open EXIF cache DB at {}: {}",
                cache_dir.display(),
                e
            )),
        ),
    }
}

/// Get the directory for storing the EXIF cache database.
fn get_cache_dir() -> Option<PathBuf> {
    // Use Tauri's app data directory if available, otherwise fall back to a local dir.
    // On Windows: %APPDATA%/<app_id>
    // On macOS: ~/Library/Application Support/<app_id>
    // On Linux: ~/.local/share/<app_id>
    if let Some(app_data) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(app_data).join("photo-exif-analyzer"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        return Some(PathBuf::from(home).join(".local/share/photo-exif-analyzer"));
    }
    None
}

/// Query the EXIF cache initialization status. Exposed as a Tauri command
/// so the frontend can warn the user when caching is unavailable (P1.7).
/// Returns `(available, error_reason)` where `error_reason` is None if
/// the cache is healthy or hasn't been queried yet.
#[tauri::command]
fn get_cache_status() -> (bool, Option<String>) {
    let available = EXIF_CACHE.is_some();
    let error = CACHE_INIT_ERROR.lock().unwrap().clone();
    (available, error)
}

#[tauri::command]
async fn scan_images_with_progress(
    window: tauri::Window,
    dir: String,
    recursive: bool,
) -> Result<Vec<ScanResult>, String> {
    *SCAN_CANCELLED.lock().unwrap() = false;
    let cancelled = Arc::clone(&SCAN_CANCELLED);

    // Use the global EXIF cache — dramatically reduces disk I/O on repeat scans
    let cache = EXIF_CACHE.as_ref().cloned();

    // Kick off cache cleanup in a BACKGROUND thread — do NOT block the scan
    // from starting. Previously `cleanup()` was called synchronously here,
    // which for 10k+ cache entries meant 10k+ `path.exists()` syscalls before
    // a single file was scanned, producing the "stuck at 0%" feeling.
    if let Some(cache_arc) = EXIF_CACHE.as_ref().cloned() {
        std::thread::spawn(move || {
            let _ = cache_arc.lock().unwrap().cleanup();
        });
    }

    // Run the blocking scan on a background thread so the Tauri main thread
    // (and thus the webview event loop) stays responsive. Progress events
    // emitted from this thread can be delivered to the frontend immediately.
    let result = tauri::async_runtime::spawn_blocking(move || {
        scan_directory_with_cache(
            &dir,
            recursive,
            cache,
            // Throttle progress emissions: wrap the raw window.emit in a
            // rate-limited layer so we don't cross the Tauri IPC boundary
            // once per file (10k files = 10k IPC calls = significant lag).
            // The wrapper guarantees a final 100% emit.
            throttled_progress(move |payload: ScanProgressPayload| {
                let _ = window.emit("scan_progress", payload);
            }),
            move || *cancelled.lock().unwrap(),
        )
    })
    .await
    .map_err(|e| format!("Scan task failed: {}", e))?;

    Ok(result)
}

/// Wrap a progress callback with a ~100ms emission throttle.
///
/// Guarantees:
///  - The very first call (0%) always passes through immediately.
///  - The final call (100%) always passes through immediately.
///  - Intermediate calls pass through at most once every MIN_PROGRESS_INTERVAL_MS,
///    dropping the "stale" percentage values that nobody will see anyway.
///
/// Without this, 10k cached files produce 10k `window.emit` IPC calls, which
/// floods the webview event loop and makes the scan *appear* slow even though
/// the backend work finishes quickly.
fn throttled_progress<F: Fn(ScanProgressPayload) + Send + Sync + 'static>(
    callback: F,
) -> impl Fn(ScanProgressPayload) + Send + Sync + 'static {
    const MIN_PROGRESS_INTERVAL_MS: u128 = 100;
    let last_emit = Arc::new(Mutex::new(
        std::time::Instant::now()
            .checked_sub(std::time::Duration::from_millis(
                MIN_PROGRESS_INTERVAL_MS as u64,
            ))
            .unwrap_or_else(std::time::Instant::now),
    ));

    move |payload: ScanProgressPayload| {
        let should_emit = {
            let mut guard = last_emit.lock().unwrap();
            let now = std::time::Instant::now();
            // Always emit 0% (first call), 100% (final call), or if enough
            // wall-clock time has elapsed since the last emission.
            if payload.percentage <= 0.0 || payload.percentage >= 100.0
                || now.duration_since(*guard).as_millis() >= MIN_PROGRESS_INTERVAL_MS
            {
                *guard = now;
                true
            } else {
                false
            }
        };
        if should_emit {
            callback(payload);
        }
    }
}

#[tauri::command]
fn cancel_scan() {
    *SCAN_CANCELLED.lock().unwrap() = true;
}

#[tauri::command]
fn get_camera_stats(results: Vec<ScanResult>) -> CameraStats {
    calculate_camera_stats(&results)
}

#[tauri::command]
fn get_lens_stats(results: Vec<ScanResult>) -> LensStats {
    calculate_lens_stats(&results)
}

#[tauri::command]
fn get_focal_length_stats(results: Vec<ScanResult>) -> FocalLengthStats {
    calculate_focal_length_stats(&results)
}

#[tauri::command]
fn filter_images(results: Vec<ScanResult>, criteria: FilterCriteria) -> Vec<ScanResult> {
    filter_results(&results, &criteria)
}

/// Compute all three statistic groups (camera / lens / focal length) in a
/// SINGLE command invocation.
///
/// The previous frontend code called `get_camera_stats`, `get_lens_stats`,
/// and `get_focal_length_stats` as THREE separate `invoke()` calls. Each
/// call serialized the full `results` array (potentially thousands of
/// entries) across the Tauri IPC boundary, triplicating the wire cost. This
/// merged command ships the array once and computes all three stats in a
/// single pass over the data inside `calculate_all_stats`.
#[tauri::command]
fn get_all_stats(results: Vec<ScanResult>) -> AllStats {
    calculate_all_stats(&results)
}

/// Asynchronously clean up cached data for deleted files.
/// Runs in a background thread to avoid blocking the delete operation.
fn cleanup_caches_async(paths: Vec<String>) {
    std::thread::spawn(move || {
        for path_str in paths.iter() {
            let path = Path::new(path_str);

            // Clean up fixed-size thumbnail (150x150)
            let _ = delete_thumbnail(path);
            // Clean up all size-aware disk caches ({stem}_{hash}_{size}.jpg)
            let _ = delete_all_size_caches(path);

            // Clean up EXIF cache
            if let Some(cache) = EXIF_CACHE.as_ref() {
                let _ = cache.lock().unwrap().remove(path);
            }
        }
    });
}

#[tauri::command]
fn delete_image(path: String) -> Result<(), String> {
    let result = delete_file(&path);
    if result.is_ok() {
        cleanup_caches_async(vec![path]);
    }
    result
}

#[tauri::command]
fn delete_images_with_progress(window: tauri::Window, paths: Vec<String>) -> Vec<Result<(), String>> {
    let total = paths.len() as f64;
    let mut results = Vec::with_capacity(paths.len());
    let mut to_cleanup: Vec<String> = Vec::new();

    for (i, path_str) in paths.iter().enumerate() {
        let path = Path::new(path_str);

        let file_result = delete_file(path);

        if file_result.is_ok() {
            to_cleanup.push(path_str.clone());
        }

        results.push(file_result);

        let progress = ((i + 1) as f64 / total) * 100.0;
        let _ = window.emit("delete_progress", progress);
    }

    // Offload cache cleanup to background thread so UI returns immediately
    if !to_cleanup.is_empty() {
        cleanup_caches_async(to_cleanup);
    }

    results
}

#[tauri::command]
async fn get_image_data(path: String, max_size: Option<u32>) -> Result<String, String> {
    let path = std::path::PathBuf::from(path);
    // Command-layer clamp. Also clamped again inside `get_image_jpeg_bytes`
    // as defense-in-depth for non-command callers; the redundancy is
    // intentional (public surface defense + module defense).
    let size = max_size
        .filter(|&s| s > 0)
        .unwrap_or(800)
        .min(exif::thumbnail::MAX_ALLOWED_THUMBNAIL_SIZE);
    tauri::async_runtime::spawn_blocking(move || {
        exif::thumbnail::get_image_base64_cached(&path, size)
    })
    .await
    .map_err(|e| format!("Image decode task failed: {}", e))?
}

#[derive(Debug, Serialize)]
pub struct ExportData {
    pub timestamp: String,
    pub total_images: usize,
    pub statistics: ExportStatistics,
    pub images: Vec<ExportImage>,
}

#[derive(Debug, Serialize)]
pub struct ExportStatistics {
    pub cameras: CameraStats,
    pub lenses: LensStats,
    pub focal_length: FocalLengthStats,
}

#[derive(Debug, Serialize)]
pub struct ExportImage {
    pub path: String,
    pub filename: String,
    pub size: u64,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<f64>,
    pub iso: Option<u32>,
    pub datetime: Option<String>,
}

#[tauri::command]
fn export_statistics(results: Vec<ScanResult>) -> Result<ExportData, String> {
    let camera_stats = calculate_camera_stats(&results);
    let lens_stats = calculate_lens_stats(&results);
    let focal_length_stats = calculate_focal_length_stats(&results);

    let images: Vec<ExportImage> = results
        .into_iter()
        .map(|r| {
            let filename = r.path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            ExportImage {
                path: r.path.to_string_lossy().to_string(),
                filename,
                size: r.file_size,
                camera: r.exif.camera_name(),
                lens: r.exif.lens_model,
                focal_length: r.exif.focal_length,
                aperture: r.exif.aperture,
                shutter_speed: r.exif.exposure_time,
                iso: r.exif.iso,
                datetime: r.exif.datetime_original,
            }
        })
        .collect();

    Ok(ExportData {
        timestamp: chrono::Utc::now().to_rfc3339(),
        total_images: images.len(),
        statistics: ExportStatistics {
            cameras: camera_stats,
            lenses: lens_stats,
            focal_length: focal_length_stats,
        },
        images,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_images_with_progress,
            cancel_scan,
            get_camera_stats,
            get_lens_stats,
            get_focal_length_stats,
            get_all_stats,
            filter_images,
            delete_image,
            delete_images_with_progress,
            get_image_data,
            export_statistics,
            get_cache_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    // NOTE: do NOT `use super::*` — it would bring the local `exif` module
    // into scope and shadow / conflict with the `exif` (kamadak-exif) crate
    // pulled in via `--extern`, producing E0659 "ambiguous name". Importing
    // only the specific items we need avoids the glob collision.
    use super::export_statistics;
    use super::init_cache_with_reason;
    use crate::exif::scanner::ScanResult;
    use crate::exif::ExifData;
    use std::path::PathBuf;

    /// Build a ScanResult with the given path and EXIF fields.
    /// `file_size` is derived from the path string length so different
    /// images produce distinguishable sizes without extra parameters.
    #[allow(clippy::too_many_arguments)]
    fn make_result(
        path: &str,
        make: Option<&str>,
        model: Option<&str>,
        lens: Option<&str>,
        focal: Option<f64>,
        aperture: Option<f64>,
        iso: Option<u32>,
        exposure: Option<f64>,
        datetime: Option<&str>,
    ) -> ScanResult {
        ScanResult {
            path: PathBuf::from(path),
            exif: ExifData {
                make: make.map(String::from),
                model: model.map(String::from),
                lens_model: lens.map(String::from),
                focal_length: focal,
                aperture,
                iso,
                exposure_time: exposure,
                datetime_original: datetime.map(String::from),
                ..Default::default()
            },
            file_size: path.len() as u64,
            error: None,
        }
    }

    #[test]
    fn test_export_statistics_empty() {
        let results: Vec<ScanResult> = Vec::new();
        let data = export_statistics(results).expect("export should succeed on empty input");

        assert_eq!(data.total_images, 0);
        assert!(data.images.is_empty());
        assert_eq!(data.statistics.cameras.total, 0);
        assert_eq!(data.statistics.lenses.total, 0);
        assert_eq!(data.statistics.focal_length.total, 0);
        // timestamp should parse as a valid RFC3339 timestamp
        assert!(chrono::DateTime::parse_from_rfc3339(&data.timestamp).is_ok());
    }

    #[test]
    fn test_export_statistics_maps_full_exif() {
        let results = vec![make_result(
            "/photos/IMG_001.jpg",
            Some("Canon"),
            Some("EOS R5"),
            Some("RF 50mm F1.2"),
            Some(50.0),
            Some(1.2),
            Some(100),
            Some(0.002),
            Some("2024-01-15T10:30:00"),
        )];

        let data = export_statistics(results).expect("export should succeed");

        assert_eq!(data.total_images, 1);
        assert_eq!(data.images.len(), 1);

        let img = &data.images[0];
        assert_eq!(img.path, "/photos/IMG_001.jpg");
        assert_eq!(img.filename, "IMG_001.jpg");
        assert_eq!(img.camera.as_deref(), Some("Canon EOS R5"));
        assert_eq!(img.lens.as_deref(), Some("RF 50mm F1.2"));
        assert_eq!(img.focal_length, Some(50.0));
        assert_eq!(img.aperture, Some(1.2));
        assert_eq!(img.iso, Some(100));
        assert_eq!(img.shutter_speed, Some(0.002));
        assert_eq!(img.datetime.as_deref(), Some("2024-01-15T10:30:00"));

        // Aggregated stats
        assert_eq!(data.statistics.cameras.total, 1);
        assert_eq!(data.statistics.cameras.cameras[0].name, "Canon EOS R5");
        assert_eq!(data.statistics.lenses.total, 1);
        assert_eq!(data.statistics.focal_length.total, 1);
    }

    #[test]
    fn test_export_statistics_camera_with_only_make() {
        // make present, model missing → camera should be the make alone
        let results = vec![make_result(
            "/photos/sigma.jpg",
            Some("Sigma"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )];

        let data = export_statistics(results).expect("export should succeed");
        assert_eq!(data.images[0].camera.as_deref(), Some("Sigma"));
        assert_eq!(data.statistics.cameras.total, 1);
    }

    #[test]
    fn test_export_statistics_camera_with_only_model() {
        // model present, make missing → the `camera` field on ExportImage is
        // the model alone (see match in export_statistics). NOTE:
        // `calculate_camera_stats` only counts results whose `make` is
        // present, so `cameras.total` stays 0 for this case — this is the
        // pre-existing aggregation behavior and is intentionally preserved
        // here rather than "fixed" in this refactor.
        let results = vec![make_result(
            "/photos/iphone.jpg",
            None,
            Some("iPhone 15 Pro"),
            None,
            None,
            None,
            None,
            None,
            None,
        )];

        let data = export_statistics(results).expect("export should succeed");
        assert_eq!(data.images[0].camera.as_deref(), Some("iPhone 15 Pro"));
        assert_eq!(data.statistics.cameras.total, 0);
        assert!(data.statistics.cameras.cameras.is_empty());
    }

    #[test]
    fn test_export_statistics_camera_no_make_no_model() {
        // Both make and model missing → camera should be None
        let results = vec![make_result(
            "/photos/no_exif.jpg",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )];

        let data = export_statistics(results).expect("export should succeed");
        assert!(data.images[0].camera.is_none());
        assert_eq!(data.statistics.cameras.total, 0);
        assert!(data.statistics.cameras.cameras.is_empty());
    }

    #[test]
    fn test_export_statistics_filename_extraction() {
        // Verify filename is extracted from a Windows-style path with a
        // backslash separator (regression guard for cross-platform paths).
        let results = vec![make_result(
            "C:\\Users\\Alice\\Pictures\\vacation.CR2",
            Some("Canon"),
            Some("EOS R6"),
            None,
            None,
            None,
            None,
            None,
            None,
        )];

        let data = export_statistics(results).expect("export should succeed");
        assert_eq!(data.images[0].filename, "vacation.CR2");
    }

    #[test]
    fn test_export_statistics_filename_unknown_for_no_filename() {
        // A root path (just `/`) has no file_name component on any platform
        // → the export falls back to "unknown" rather than panicking.
        // NOTE: a trailing separator like `/photos/` is NOT sufficient —
        // Rust's Path strips trailing separators and returns the last
        // normal component ("photos"), so we use the bare root here.
        let results = vec![make_result(
            "/",
            Some("Canon"),
            Some("EOS R5"),
            None,
            None,
            None,
            None,
            None,
            None,
        )];

        let data = export_statistics(results).expect("export should succeed");
        assert_eq!(data.images[0].filename, "unknown");
    }

    #[test]
    fn test_export_statistics_aggregates_multiple_images() {
        let results = vec![
            make_result(
                "/photos/a.jpg",
                Some("Canon"),
                Some("EOS R5"),
                Some("RF 50mm"),
                Some(50.0),
                None,
                None,
                None,
                None,
            ),
            make_result(
                "/photos/b.jpg",
                Some("Canon"),
                Some("EOS R5"),
                Some("RF 50mm"),
                Some(50.0),
                None,
                None,
                None,
                None,
            ),
            make_result(
                "/photos/c.jpg",
                Some("Nikon"),
                Some("Z6"),
                Some("NIKKOR Z 24-70"),
                Some(24.0),
                None,
                None,
                None,
                None,
            ),
        ];

        let data = export_statistics(results).expect("export should succeed");

        assert_eq!(data.total_images, 3);
        assert_eq!(data.images.len(), 3);

        // Camera stats: 2 Canon EOS R5 + 1 Nikon Z6
        assert_eq!(data.statistics.cameras.total, 3);
        assert_eq!(data.statistics.cameras.cameras.len(), 2);
        assert_eq!(data.statistics.cameras.cameras[0].name, "Canon EOS R5");
        assert_eq!(data.statistics.cameras.cameras[0].count, 2);

        // Lens stats
        assert_eq!(data.statistics.lenses.total, 3);
        assert_eq!(data.statistics.lenses.lenses.len(), 2);

        // Focal length stats: 2 at 50mm + 1 at 24mm
        assert_eq!(data.statistics.focal_length.total, 3);
    }

    // ---- P1.7: EXIF cache initialization failure handling ----

    #[test]
    fn test_init_cache_with_reason_returns_error_when_env_missing() {
        // P1.7: when neither APPDATA nor HOME is set, init_cache_with_reason
        // must return (None, Some(reason)) — NOT silently (None, None).
        // We can't fully control env vars in a test, but we CAN assert that
        // the error branch is reachable: by clearing both env vars (saving
        // and restoring them) and verifying the reason is non-empty.
        //
        // NOTE: This test is best-effort. If the test runner has neither
        // APPDATA nor HOME set natively, the "missing env" path is exercised
        // directly. If either is set, we temporarily remove it; the test
        // is still valid because `init_cache_with_reason` re-reads env.
        let saved_appdata = std::env::var_os("APPDATA");
        let saved_home = std::env::var_os("HOME");

        std::env::remove_var("APPDATA");
        std::env::remove_var("HOME");

        let (cache, error) = init_cache_with_reason();

        // Restore env vars regardless of test outcome
        if let Some(v) = saved_appdata { std::env::set_var("APPDATA", v); }
        if let Some(v) = saved_home { std::env::set_var("HOME", v); }

        assert!(cache.is_none(), "cache must be None when env vars are missing");
        let reason = error.expect("error reason must be Some when init fails");
        assert!(
            !reason.is_empty(),
            "error reason must not be empty (was: {:?})",
            reason
        );
        assert!(
            reason.to_lowercase().contains("app data directory")
                || reason.to_lowercase().contains("appdata")
                || reason.to_lowercase().contains("home"),
            "error reason should mention the missing env var, got: {:?}",
            reason
        );
    }

    #[test]
    fn test_init_cache_with_reason_returns_cache_on_success() {
        // P1.7: when env vars ARE set and the cache dir is writable,
        // init_cache_with_reason must return (Some(cache), None).
        // Uses a temp dir as APPDATA to avoid polluting real app data.
        let temp = tempfile::TempDir::new().unwrap();
        std::env::set_var("APPDATA", temp.path());

        let (cache, error) = init_cache_with_reason();

        // Restore: clear our override so other tests aren't affected.
        // (The real APPDATA is restored by the test harness or OS env.)
        std::env::remove_var("APPDATA");

        assert!(cache.is_some(), "cache should initialize successfully in a writable temp dir");
        assert!(
            error.is_none(),
            "error reason must be None on success, got: {:?}",
            error
        );
    }
}
