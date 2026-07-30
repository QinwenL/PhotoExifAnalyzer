pub mod exif;

use exif::scanner::{scan_directory, ScanResult};
use exif::stats::{
    calculate_camera_stats, calculate_focal_length_stats, calculate_lens_stats,
    filter_results, CameraStats, FilterCriteria, FocalLengthStats, LensStats,
};
use exif::file_ops::{delete_file, delete_files};

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
