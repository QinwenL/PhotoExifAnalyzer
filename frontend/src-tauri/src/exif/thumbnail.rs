use std::path::{Path, PathBuf};

/// Thumbnail size in pixels
const THUMBNAIL_SIZE: u32 = 150;

/// Get or create thumbnail for an image
///
/// # Arguments
/// * `path` - Path to the original image
///
/// # Returns
/// * `Result<PathBuf, String>` - Path to the thumbnail file
pub fn get_thumbnail_path(path: &Path) -> Result<PathBuf, String> {
    let thumb_dir = get_thumbnail_dir(path)?;
    let thumb_name = get_thumbnail_name(path)?;
    let thumb_path = thumb_dir.join(&thumb_name);

    if thumb_path.exists() {
        return Ok(thumb_path);
    }

    // Generate thumbnail
    generate_thumbnail(path, &thumb_path)?;

    Ok(thumb_path)
}

/// Get the thumbnail directory for a given image path
fn get_thumbnail_dir(image_path: &Path) -> Result<PathBuf, String> {
    let parent = image_path
        .parent()
        .ok_or_else(|| format!("Cannot get parent directory: {}", image_path.display()))?;

    let thumb_dir = parent.join(".thumbnails");

    if !thumb_dir.exists() {
        std::fs::create_dir_all(&thumb_dir)
            .map_err(|e| format!("Failed to create thumbnail directory: {}", e))?;
    }

    Ok(thumb_dir)
}

/// Generate thumbnail filename from original filename
fn get_thumbnail_name(path: &Path) -> Result<String, String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Invalid filename: {}", path.display()))?;

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("jpg");

    // Create a hash of the full path to avoid collisions
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();

    Ok(format!("{}_{}.{ext}", stem, hash))
}

/// Generate thumbnail from image file
fn generate_thumbnail(input_path: &Path, output_path: &Path) -> Result<(), String> {
    let img = image::open(input_path)
        .map_err(|e| format!("Failed to open image: {}", e))?;

    let thumbnail = img.resize(THUMBNAIL_SIZE, THUMBNAIL_SIZE, image::imageops::FilterType::Lanczos3);

    thumbnail
        .save(output_path)
        .map_err(|e| format!("Failed to save thumbnail: {}", e))?;

    Ok(())
}

/// Get image data as base64 string for display in frontend
///
/// # Arguments
/// * `path` - Path to the image
/// * `max_size` - Maximum dimension (width or height)
///
/// # Returns
/// * `Result<String, String>` - Base64 encoded image data
pub fn get_image_base64(path: &Path, max_size: u32) -> Result<String, String> {
    let img = image::open(path)
        .map_err(|e| format!("Failed to open image: {}", e))?;

    let resized = img.resize(max_size, max_size, image::imageops::FilterType::Lanczos3);

    let mut buffer = std::io::Cursor::new(Vec::new());
    resized
        .write_to(&mut buffer, image::ImageOutputFormat::Jpeg(85))
        .map_err(|e| format!("Failed to encode image: {}", e))?;

    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(buffer.into_inner());

    Ok(format!("data:image/jpeg;base64,{}", encoded))
}

/// Check if thumbnail exists
pub fn thumbnail_exists(path: &Path) -> bool {
    get_thumbnail_path(path)
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Delete thumbnail for an image
pub fn delete_thumbnail(path: &Path) -> Result<(), String> {
    let thumb_path = get_thumbnail_path(path)?;
    if thumb_path.exists() {
        std::fs::remove_file(&thumb_path)
            .map_err(|e| format!("Failed to delete thumbnail: {}", e))?;
    }
    Ok(())
}

/// Clear all thumbnails in a directory
pub fn clear_thumbnails(dir: &Path) -> Result<usize, String> {
    let thumb_dir = dir.join(".thumbnails");
    if !thumb_dir.exists() {
        return Ok(0);
    }

    let mut count = 0;
    for entry in std::fs::read_dir(&thumb_dir)
        .map_err(|e| format!("Failed to read thumbnail directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();

        if path.is_file() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete thumbnail: {}", e))?;
            count += 1;
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_jpeg(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        // Minimal valid JPEG with image data
        let jpeg = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
        ];
        let mut file = File::create(&path).unwrap();
        file.write_all(&jpeg).unwrap();
        path
    }

    #[test]
    fn test_get_thumbnail_dir() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = temp_dir.path().join("photo.jpg");

        let thumb_dir = get_thumbnail_dir(&image_path).unwrap();
        assert!(thumb_dir.exists());
        assert_eq!(thumb_dir, temp_dir.path().join(".thumbnails"));
    }

    #[test]
    fn test_get_thumbnail_name() {
        let path = Path::new("/photos/2024/img_001.jpg");
        let name = get_thumbnail_name(path).unwrap();
        assert!(name.starts_with("img_001_"));
        assert!(name.ends_with(".jpg"));
    }

    #[test]
    fn test_thumbnail_exists() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = create_test_jpeg(temp_dir.path(), "test.jpg");

        // Initially no thumbnail
        assert!(!thumbnail_exists(&image_path));

        // Create thumbnail directory (but no thumbnail yet)
        let thumb_dir = temp_dir.path().join(".thumbnails");
        std::fs::create_dir_all(&thumb_dir).unwrap();

        // Still no thumbnail
        assert!(!thumbnail_exists(&image_path));
    }

    #[test]
    fn test_clear_thumbnails() {
        let temp_dir = TempDir::new().unwrap();
        let thumb_dir = temp_dir.path().join(".thumbnails");
        std::fs::create_dir_all(&thumb_dir).unwrap();

        // Create some fake thumbnails
        File::create(thumb_dir.join("thumb1.jpg")).unwrap();
        File::create(thumb_dir.join("thumb2.jpg")).unwrap();

        let count = clear_thumbnails(temp_dir.path()).unwrap();
        assert_eq!(count, 2);
        assert!(!thumb_dir.join("thumb1.jpg").exists());
    }
}
