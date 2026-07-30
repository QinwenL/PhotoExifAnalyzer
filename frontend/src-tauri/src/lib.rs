pub mod exif;

use std::path::Path;
use std::sync::{Arc, Mutex};

use exif::scanner::{scan_directory, scan_directory_with_callback, ScanResult};
use exif::stats::{
    calculate_camera_stats, calculate_focal_length_stats, calculate_lens_stats,
    filter_results, CameraStats, FilterCriteria, FocalLengthStats, LensStats,
};
use exif::file_ops::{delete_file, delete_files};
use exif::thumbnail::{get_thumbnail_path, get_image_base64};

lazy_static::lazy_static! {
    static ref SCAN_CANCELLED: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
}

#[tauri::command]
fn scan_images(dir: String, recursive: bool) -> Vec<ScanResult> {
    *SCAN_CANCELLED.lock().unwrap() = false;
    scan_directory(&dir, recursive)
}

#[tauri::command]
fn scan_images_with_progress(dir: String, recursive: bool) -> Vec<ScanResult> {
    *SCAN_CANCELLED.lock().unwrap() = false;
    let cancelled = Arc::clone(&SCAN_CANCELLED);

    scan_directory_with_callback(
        &dir,
        recursive,
        |_| {},
        move || *cancelled.lock().unwrap(),
    )
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

#[tauri::command]
fn delete_image(path: String) -> Result<(), String> {
    delete_file(&path)
}

#[tauri::command]
fn delete_images(paths: Vec<String>) -> Vec<Result<(), String>> {
    delete_files(&paths)
}

#[tauri::command]
fn delete_images_with_progress(paths: Vec<String>) -> Vec<Result<(), String>> {
    delete_files(&paths)
}

#[tauri::command]
fn get_thumbnail(path: String) -> Result<String, String> {
    let path = Path::new(&path);
    let thumb_path = get_thumbnail_path(path)?;
    Ok(thumb_path.to_string_lossy().to_string())
}

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
