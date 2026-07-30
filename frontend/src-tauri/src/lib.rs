pub mod exif;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use exif::cache::ExifCache;
use exif::scanner::{scan_directory, scan_directory_with_cache, ScanResult};
use exif::stats::{
    calculate_camera_stats, calculate_focal_length_stats, calculate_lens_stats,
    filter_results, CameraStats, FilterCriteria, FocalLengthStats, LensStats,
};
use serde::Serialize;
use exif::file_ops::{delete_file, delete_files};
use exif::thumbnail::{delete_thumbnail, delete_all_size_caches, get_thumbnail_path};

lazy_static::lazy_static! {
    static ref SCAN_CANCELLED: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    /// Global EXIF cache stored in the app's data directory.
    /// Initialized lazily on first scan; shared across all scan operations.
    static ref EXIF_CACHE: Option<Arc<Mutex<ExifCache>>> = init_cache();
}

/// Initialize the EXIF cache in the user's app data directory.
/// Returns None if the directory cannot be determined or cache creation fails.
fn init_cache() -> Option<Arc<Mutex<ExifCache>>> {
    let cache_dir = get_cache_dir()?;
    // Ensure the cache directory exists
    if std::fs::create_dir_all(&cache_dir).is_err() {
        return None;
    }
    match ExifCache::new(&cache_dir) {
        Ok(cache) => Some(Arc::new(Mutex::new(cache))),
        Err(_) => None,
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

#[tauri::command]
fn scan_images(dir: String, recursive: bool) -> Vec<ScanResult> {
    *SCAN_CANCELLED.lock().unwrap() = false;
    scan_directory(&dir, recursive)
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

    // Remove dead entries before scanning
    if let Some(cache) = EXIF_CACHE.as_ref() {
        let _ = cache.lock().unwrap().cleanup();
    }

    // Run the blocking scan on a background thread so the Tauri main thread
    // (and thus the webview event loop) stays responsive. Progress events
    // emitted from this thread can be delivered to the frontend immediately.
    let result = tauri::async_runtime::spawn_blocking(move || {
        scan_directory_with_cache(
            &dir,
            recursive,
            cache,
            move |pct| {
                let _ = window.emit("scan_progress", pct);
            },
            move || *cancelled.lock().unwrap(),
        )
    })
    .await
    .map_err(|e| format!("Scan task failed: {}", e))?;

    Ok(result)
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
fn delete_images(paths: Vec<String>) -> Vec<Result<(), String>> {
    let results = delete_files(&paths);
    let to_cleanup: Vec<String> = paths
        .iter()
        .zip(results.iter())
        .filter(|(_, r)| r.is_ok())
        .map(|(p, _)| p.clone())
        .collect();
    if !to_cleanup.is_empty() {
        cleanup_caches_async(to_cleanup);
    }
    results
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
fn get_thumbnail(path: String) -> Result<String, String> {
    let path = Path::new(&path);
    let thumb_path = get_thumbnail_path(path)?;
    Ok(thumb_path.to_string_lossy().to_string())
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
            
            let camera = match (&r.exif.make, &r.exif.model) {
                (Some(make), Some(model)) => Some(format!("{} {}", make, model)),
                (Some(make), None) => Some(make.clone()),
                (None, Some(model)) => Some(model.clone()),
                _ => None,
            };

            ExportImage {
                path: r.path.to_string_lossy().to_string(),
                filename,
                size: r.file_size,
                camera,
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
            scan_images,
            scan_images_with_progress,
            cancel_scan,
            get_camera_stats,
            get_lens_stats,
            get_focal_length_stats,
            filter_images,
            delete_image,
            delete_images,
            delete_images_with_progress,
            get_thumbnail,
            get_image_data,
            export_statistics,
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
}
