pub mod exif;

use std::path::Path;

use exif::scanner::{scan_directory, ScanResult};
use exif::stats::{
    calculate_camera_stats, calculate_focal_length_stats, calculate_lens_stats,
    filter_results, CameraStats, FilterCriteria, FocalLengthStats, LensStats,
};
use exif::file_ops::{delete_file, delete_files};
use exif::thumbnail::{get_thumbnail_path, get_image_base64};

/// Scan a directory for images
#[tauri::command]
fn scan_images(dir: String, recursive: bool) -> Vec<ScanResult> {
    scan_directory(&dir, recursive)
}

/// Get camera statistics
#[tauri::command]
fn get_camera_stats(results: Vec<ScanResult>) -> CameraStats {
    calculate_camera_stats(&results)
}

/// Get lens statistics
#[tauri::command]
fn get_lens_stats(results: Vec<ScanResult>) -> LensStats {
    calculate_lens_stats(&results)
}

/// Get focal length statistics
#[tauri::command]
fn get_focal_length_stats(results: Vec<ScanResult>) -> FocalLengthStats {
    calculate_focal_length_stats(&results)
}

/// Filter scan results
#[tauri::command]
fn filter_images(results: Vec<ScanResult>, criteria: FilterCriteria) -> Vec<ScanResult> {
    filter_results(&results, &criteria)
}

/// Delete a single file (move to recycle bin)
#[tauri::command]
fn delete_image(path: String) -> Result<(), String> {
    delete_file(&path)
}

/// Delete multiple files (move to recycle bin)
#[tauri::command]
fn delete_images(paths: Vec<String>) -> Vec<Result<(), String>> {
    delete_files(&paths)
}

/// Get thumbnail path for an image
#[tauri::command]
fn get_thumbnail(path: String) -> Result<String, String> {
    let path = Path::new(&path);
    let thumb_path = get_thumbnail_path(path)?;
    Ok(thumb_path.to_string_lossy().to_string())
}

/// Get image data as base64 for display
#[tauri::command]
fn get_image_data(path: String, max_size: Option<u32>) -> Result<String, String> {
    let path = Path::new(&path);
    let size = max_size.unwrap_or(800);
    get_image_base64(path, size)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan_images,
            get_camera_stats,
            get_lens_stats,
            get_focal_length_stats,
            filter_images,
            delete_image,
            delete_images,
            get_thumbnail,
            get_image_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
