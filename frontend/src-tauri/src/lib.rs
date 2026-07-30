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
use exif::thumbnail::{delete_thumbnail, get_thumbnail_path};

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

            // Clean up thumbnail cache
            let _ = delete_thumbnail(path);

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
    let size = max_size.unwrap_or(800);
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
