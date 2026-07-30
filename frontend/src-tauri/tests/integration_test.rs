use photo_exif_analyzer_lib::exif;
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

/// Create a minimal valid JPEG file
fn create_test_image(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    // Minimal valid JPEG: SOI + APP0 + EOI
    let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9];
    let mut file = File::create(&path).unwrap();
    file.write_all(&jpeg).unwrap();
    path
}

#[test]
fn test_full_scan_pipeline() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create test images
    create_test_image(temp_dir.path(), "photo1.jpg");
    create_test_image(temp_dir.path(), "photo2.jpeg");
    create_test_image(temp_dir.path(), "document.txt"); // Should be ignored
    
    // Scan directory
    let results = exif::scanner::scan_directory(temp_dir.path(), true);
    
    // Should find 2 images (ignoring .txt)
    assert_eq!(results.len(), 2);
    
    // All results should have valid paths
    for result in &results {
        assert!(result.path.exists());
        assert!(result.file_size > 0);
    }
}

#[test]
fn test_scan_recursive() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create images in root and subdirectory
    create_test_image(temp_dir.path(), "root.jpg");
    
    let sub_dir = temp_dir.path().join("subdir");
    fs::create_dir(&sub_dir).unwrap();
    create_test_image(&sub_dir, "sub.jpg");
    
    let sub_sub_dir = sub_dir.join("subsubdir");
    fs::create_dir(&sub_sub_dir).unwrap();
    create_test_image(&sub_sub_dir, "subsub.jpg");
    
    // Recursive scan should find all 3
    let results = exif::scanner::scan_directory(temp_dir.path(), true);
    assert_eq!(results.len(), 3);
    
    // Non-recursive should find only 1
    let results = exif::scanner::scan_directory(temp_dir.path(), false);
    assert_eq!(results.len(), 1);
}

#[test]
fn test_statistics_calculation() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create test images
    create_test_image(temp_dir.path(), "photo1.jpg");
    create_test_image(temp_dir.path(), "photo2.jpg");
    
    // Scan
    let results = exif::scanner::scan_directory(temp_dir.path(), true);
    assert!(!results.is_empty());
    
    // Calculate stats (should not panic, even with no EXIF data in test images)
    let camera_stats = exif::stats::calculate_camera_stats(&results);
    let lens_stats = exif::stats::calculate_lens_stats(&results);
    let focal_stats = exif::stats::calculate_focal_length_stats(&results);

    // Stats should be valid structures (test images are minimal stubs with no EXIF data, so totals = 0)
    assert!(camera_stats.cameras.is_empty());
    assert!(lens_stats.lenses.is_empty());
    assert!(focal_stats.ranges.is_empty());
}

#[test]
fn test_filter_and_mode() {
    use photo_exif_analyzer_lib::exif::stats::FilterCriteria;
    
    let temp_dir = TempDir::new().unwrap();
    create_test_image(temp_dir.path(), "photo1.jpg");
    
    let results = exif::scanner::scan_directory(temp_dir.path(), true);
    
    // Filter with empty criteria should return all
    let criteria = FilterCriteria::default();
    
    let filtered = exif::stats::filter_results(&results, &criteria);
    assert_eq!(filtered.len(), results.len());
}

#[test]
fn test_thumbnail_operations() {
    let temp_dir = TempDir::new().unwrap();
    create_test_image(temp_dir.path(), "photo.jpg");
    
    let results = exif::scanner::scan_directory(temp_dir.path(), true);
    assert_eq!(results.len(), 1);
    
    let path = &results[0].path;
    
    // Thumbnail should not exist initially
    assert!(!exif::thumbnail::thumbnail_exists(path));
}

#[test]
fn test_file_operations() {
    let temp_dir = TempDir::new().unwrap();
    create_test_image(temp_dir.path(), "photo.jpg");
    
    let path = temp_dir.path().join("photo.jpg");
    
    // File should exist
    assert!(exif::file_ops::file_exists(&path));
    
    // Get file size
    let size = exif::file_ops::file_size(&path);
    assert!(size.is_ok());
    assert!(size.unwrap() > 0);
    
    // Delete to trash
    let result = exif::file_ops::delete_file(&path);
    assert!(result.is_ok());
    
    // File should not exist after trash
    assert!(!exif::file_ops::file_exists(&path));
}

#[test]
fn test_batch_delete() {
    let temp_dir = TempDir::new().unwrap();
    create_test_image(temp_dir.path(), "photo1.jpg");
    create_test_image(temp_dir.path(), "photo2.jpg");
    create_test_image(temp_dir.path(), "photo3.jpg");
    
    let results = exif::scanner::scan_directory(temp_dir.path(), true);
    assert_eq!(results.len(), 3);
    
    // Delete all
    let paths: Vec<&std::path::Path> = results.iter().map(|r| r.path.as_path()).collect();
    let results = exif::file_ops::delete_files(&paths);
    
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|r| r.is_ok()));
}

#[test]
fn test_cache_operations() {
    let temp_dir = TempDir::new().unwrap();
    create_test_image(temp_dir.path(), "photo.jpg");
    
    let results = exif::scanner::scan_directory(temp_dir.path(), true);
    assert_eq!(results.len(), 1);
    
    // Initialize cache
    let cache = exif::cache::ExifCache::new(temp_dir.path());
    assert!(cache.is_ok());
    
    let cache = cache.unwrap();
    
    // Set and get
    let path = &results[0].path;
    let exif_data = results[0].exif.clone();
    
    cache.set(path, &exif_data).unwrap();
    let cached = cache.get(path);
    assert!(cached.is_some());
    
    // Remove
    cache.remove(path).unwrap();
    let cached = cache.get(path);
    assert!(cached.is_none());
}
