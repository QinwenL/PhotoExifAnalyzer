pub mod exif;

use std::path::Path;
use std::sync::{Arc, Mutex};

use exif::scanner::{scan_directory, scan_directory_with_callback, ScanResult};
use exif::stats::{
    calculate_camera_stats, calculate_focal_length_stats, calculate_lens_stats,
    filter_results, CameraStats, FilterCriteria, FocalLengthStats, LensStats,
};
use serde::Serialize;
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
    delete_files_with_progress_callback(&paths, |progress| {
        if let Some(window) = tauri::Window::get_by_label("main") {
            let _ = window.emit("delete_progress", progress);
        }
    })
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
