use std::path::{Path, PathBuf};

const THUMBNAIL_SIZE: u32 = 150;

const RAW_EXTENSIONS: &[&str] = &[
    "cr2", "cr3", "crw",
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
    "iiq",
];

const HEIC_EXTENSIONS: &[&str] = &["heic", "heif", "hif", "heics", "heifs"];

pub fn is_raw_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_lowercase();
            RAW_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

pub fn is_heic_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_lowercase();
            HEIC_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

fn open_image(path: &Path) -> Result<image::DynamicImage, String> {
    if is_heic_extension(path) {
        #[cfg(target_os = "windows")]
        {
            return crate::exif::heic::decode(path);
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Err(
                "HEIC/HEIF decoding is only supported on Windows (via WIC). \
                 Please convert to JPEG/PNG first."
                    .to_string(),
            );
        }
    }

    match image::open(path) {
        Ok(img) => Ok(img),
        Err(e) => {
            if is_raw_extension(path) {
                return extract_raw_preview(path);
            }
            Err(format!(
                "Unsupported image format ({}): {}",
                e,
                path.display()
            ))
        }
    }
}

fn extract_raw_preview(path: &Path) -> Result<image::DynamicImage, String> {
    let file_bytes = std::fs::read(path)
        .map_err(|e| format!("Failed to read RAW file: {}", e))?;

    let soi_positions = find_all_jpeg_soi(&file_bytes);

    if soi_positions.is_empty() {
        return Err(
            "No embedded JPEG preview found in RAW file (no SOI markers). \
             Thumbnail generation for this RAW format is not yet supported."
                .to_string(),
        );
    }

    let mut best_jpeg: Option<&[u8]> = None;
    let mut best_size: usize = 0;

    for &soi_pos in &soi_positions {
        if let Some(eoi_pos) = find_jpeg_eoi(&file_bytes, soi_pos.saturating_add(2)) {
            let jpeg_data = &file_bytes[soi_pos..=eoi_pos];
            if jpeg_data.len() > best_size {
                best_size = jpeg_data.len();
                best_jpeg = Some(jpeg_data);
            }
        }
    }

    let jpeg_data = best_jpeg.ok_or_else(|| {
        "No embedded JPEG preview found in RAW file (SOI without EOI). \
         Thumbnail generation for this RAW format is not yet supported."
            .to_string()
    })?;

    if jpeg_data.len() < 200 {
        return Err(
            "Embedded JPEG preview is too small (< 200 bytes). \
             RAW file may not contain a usable preview."
                .to_string(),
        );
    }

    let img = image::load_from_memory_with_format(jpeg_data, image::ImageFormat::Jpeg)
        .or_else(|_| {
            let temp_path = std::env::temp_dir().join(format!(
                "raw_preview_{}_{}.jpg",
                std::process::id(),
                best_size
            ));
            std::fs::write(&temp_path, jpeg_data)?;
            let result = image::open(&temp_path);
            let _ = std::fs::remove_file(&temp_path);
            result
        })
        .map_err(|e| format!("Failed to decode RAW preview JPEG: {} (size: {} bytes)", e, best_size))?;

    Ok(img)
}

fn find_all_jpeg_soi(data: &[u8]) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut search_pos = 0;

    while search_pos < data.len().saturating_sub(1) {
        match data[search_pos..]
            .windows(2)
            .position(|w| w[0] == 0xFF && w[1] == 0xD8)
        {
            Some(p) => {
                let pos = search_pos + p;
                positions.push(pos);
                search_pos = pos + 2;
            }
            None => break,
        }
    }

    positions
}

fn find_jpeg_eoi(data: &[u8], from_pos: usize) -> Option<usize> {
    if from_pos >= data.len().saturating_sub(1) {
        return None;
    }
    data[from_pos..]
        .windows(2)
        .position(|w| w[0] == 0xFF && w[1] == 0xD9)
        .map(|p| from_pos + p + 1)
}

pub fn get_thumbnail_path(path: &Path) -> Result<PathBuf, String> {
    let thumb_dir = get_thumbnail_dir(path)?;
    let thumb_name = get_thumbnail_name(path)?;
    let thumb_path = thumb_dir.join(&thumb_name);

    if thumb_path.exists() {
        if let Ok(source_meta) = path.metadata() {
            if let Ok(thumb_meta) = thumb_path.metadata() {
                if let (Ok(source_mtime), Ok(thumb_mtime)) =
                    (source_meta.modified(), thumb_meta.modified())
                {
                    if thumb_mtime >= source_mtime {
                        return Ok(thumb_path);
                    }
                }
            }
        }
    }

    generate_thumbnail(path, &thumb_path)?;
    Ok(thumb_path)
}

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

fn get_thumbnail_name(path: &Path) -> Result<String, String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Invalid filename: {}", path.display()))?;

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("jpg");

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();

    Ok(format!("{}_{}.{ext}", stem, hash))
}

fn generate_thumbnail(input_path: &Path, output_path: &Path) -> Result<(), String> {
    let img = open_image(input_path)?;

    let thumbnail =
        img.resize(THUMBNAIL_SIZE, THUMBNAIL_SIZE, image::imageops::FilterType::Lanczos3);

    thumbnail
        .save(output_path)
        .map_err(|e| format!("Failed to save thumbnail: {}", e))?;

    Ok(())
}

pub fn get_image_base64(path: &Path, max_size: u32) -> Result<String, String> {
    if max_size <= THUMBNAIL_SIZE {
        if let Ok(thumb_path) = get_thumbnail_path(path) {
            if let Ok(thumb) = image::open(&thumb_path) {
                let resized = if thumb.width() > max_size || thumb.height() > max_size {
                    thumb.resize(max_size, max_size, image::imageops::FilterType::Triangle)
                } else {
                    thumb
                };
                return encode_to_base64(&resized);
            }
        }
    }

    let img = open_image(path)?;
    let resized = if img.width() > max_size || img.height() > max_size {
        img.resize(max_size, max_size, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let _ = get_thumbnail_path(path);

    encode_to_base64(&resized)
}

fn encode_to_base64(img: &image::DynamicImage) -> Result<String, String> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buffer, image::ImageOutputFormat::Jpeg(85))
        .map_err(|e| format!("Failed to encode image: {}", e))?;

    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(buffer.into_inner());

    Ok(format!("data:image/jpeg;base64,{}", encoded))
}

pub fn thumbnail_exists(path: &Path) -> bool {
    if let Ok(thumb_dir) = get_thumbnail_dir(path) {
        if let Ok(thumb_name) = get_thumbnail_name(path) {
            let thumb_path = thumb_dir.join(thumb_name);
            return thumb_path.exists();
        }
    }
    false
}

pub fn delete_thumbnail(path: &Path) -> Result<(), String> {
    let thumb_path = get_thumbnail_path(path)?;
    if thumb_path.exists() {
        std::fs::remove_file(&thumb_path)
            .map_err(|e| format!("Failed to delete thumbnail: {}", e))?;
    }
    Ok(())
}

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

use std::num::NonZeroUsize;

const MEMORY_CACHE_CAPACITY: usize = 64;

lazy_static::lazy_static! {
    static ref IMAGE_DATA_CACHE: std::sync::Mutex<lru::LruCache<(std::path::PathBuf, u32), String>> =
        std::sync::Mutex::new(lru::LruCache::new(
            NonZeroUsize::new(MEMORY_CACHE_CAPACITY).unwrap()
        ));
}

pub(crate) fn get_data_cache_path(
    path: &std::path::Path,
    max_size: u32,
) -> Result<std::path::PathBuf, String> {
    let dir = get_thumbnail_dir(path)?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Invalid filename: {}", path.display()))?;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();
    Ok(dir.join(format!("{stem}_{hash}_{max_size}.jpg")))
}

pub fn get_image_base64_cached(
    path: &std::path::Path,
    max_size: u32,
) -> Result<String, String> {
    let key = (path.to_path_buf(), max_size);
    if let Some(data) = IMAGE_DATA_CACHE.lock().unwrap().get(&key).cloned() {
        return Ok(data);
    }
    if let Ok(disk_path) = get_data_cache_path(path, max_size) {
        if disk_path.exists() {
            match std::fs::read(&disk_path) {
                Ok(bytes) if bytes.starts_with(&[0xFF, 0xD8]) => {
                    use base64::Engine;
                    let data = format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&bytes)
                    );
                    IMAGE_DATA_CACHE.lock().unwrap().put(key.clone(), data.clone());
                    return Ok(data);
                }
                Ok(_) => {
                    let _ = std::fs::remove_file(&disk_path);
                }
                Err(_) => {
                    let _ = std::fs::remove_file(&disk_path);
                }
            }
        }
    }
    let data = get_image_base64(path, max_size)?;
    const B64_PREFIX: &str = "data:image/jpeg;base64,";
    if let Some(b64) = data.strip_prefix(B64_PREFIX) {
        use base64::Engine;
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
            if let Ok(disk_path) = get_data_cache_path(path, max_size) {
                let _ = get_thumbnail_dir(path);
                let _ = std::fs::write(&disk_path, &bytes);
            }
        }
    }
    IMAGE_DATA_CACHE.lock().unwrap().put(key, data.clone());
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_jpeg(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let img = image::RgbImage::from_pixel(100, 100, image::Rgb([128u8, 128, 128]));
        img.save(&path).expect("Failed to create test JPEG");
        path
    }

    fn create_test_png(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let img = image::RgbImage::from_pixel(50, 50, image::Rgb([200u8, 100, 50]));
        img.save(&path).expect("Failed to create test PNG");
        path
    }

    fn create_test_raw_with_jpeg_preview(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let mut data = vec![0u8; 1024];
        let jpeg_bytes = create_valid_jpeg_bytes(10, 10);
        data.extend_from_slice(&jpeg_bytes);
        std::fs::write(&path, &data).unwrap();
        path
    }

    fn create_test_raw_with_multiple_jpegs(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let mut data = vec![0u8; 1024];
        let small_jpeg = create_valid_jpeg_bytes(5, 5);
        let large_jpeg = create_valid_jpeg_bytes(50, 50);
        data.extend_from_slice(&small_jpeg);
        data.extend_from_slice(&large_jpeg);
        std::fs::write(&path, &data).unwrap();
        path
    }

    fn create_valid_jpeg_bytes(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([128u8, 128, 128]));
        let mut buffer = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buffer, image::ImageOutputFormat::Jpeg(85))
            .expect("Failed to encode test JPEG");
        buffer.into_inner()
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
        assert!(!thumbnail_exists(&image_path));
        let _ = get_thumbnail_path(&image_path).unwrap();
        assert!(thumbnail_exists(&image_path));
    }

    #[test]
    fn test_clear_thumbnails() {
        let temp_dir = TempDir::new().unwrap();
        let thumb_dir = temp_dir.path().join(".thumbnails");
        std::fs::create_dir_all(&thumb_dir).unwrap();
        std::fs::write(thumb_dir.join("thumb1.jpg"), b"dummy").unwrap();
        std::fs::write(thumb_dir.join("thumb2.jpg"), b"dummy").unwrap();
        let count = clear_thumbnails(temp_dir.path()).unwrap();
        assert_eq!(count, 2);
        assert!(!thumb_dir.join("thumb1.jpg").exists());
    }

    #[test]
    fn test_get_data_cache_path_has_size_suffix() {
        let temp_dir = TempDir::new().unwrap();
        let photo_dir = temp_dir.path().join("photos").join("2024");
        std::fs::create_dir_all(&photo_dir).unwrap();
        let image_path = photo_dir.join("img_001.jpg");
        let path = Path::new(&image_path);
        let p = get_data_cache_path(path, 200).unwrap();
        let file_name = p.file_name().unwrap().to_str().unwrap();
        assert!(file_name.contains("img_001_"));
        assert!(file_name.ends_with("_200.jpg"));
        assert!(p.parent().unwrap().ends_with(".thumbnails"));
    }

    #[test]
    fn test_cached_decode_is_consistent() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = create_test_jpeg(temp_dir.path(), "cached.jpg");
        let data1 = get_image_base64_cached(&image_path, 100).unwrap();
        assert!(data1.starts_with("data:image/jpeg;base64,"));
        let data2 = get_image_base64_cached(&image_path, 100).unwrap();
        assert_eq!(data1, data2);
        let disk = get_data_cache_path(&image_path, 100).unwrap();
        assert!(disk.exists());
    }

    #[test]
    fn test_cached_decode_respects_max_size() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = create_test_jpeg(temp_dir.path(), "sizes.jpg");
        let small = get_image_base64_cached(&image_path, 50).unwrap();
        let large = get_image_base64_cached(&image_path, 400).unwrap();
        let disk_small = get_data_cache_path(&image_path, 50).unwrap();
        let disk_large = get_data_cache_path(&image_path, 400).unwrap();
        assert_ne!(disk_small, disk_large);
        assert_ne!(small.len(), large.len());
    }

    #[test]
    fn test_corrupt_disk_cache_is_cleaned_and_redecoded() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = create_test_jpeg(temp_dir.path(), "corrupt.jpg");
        let disk_path = get_data_cache_path(&image_path, 100).unwrap();
        if get_thumbnail_dir(&image_path).is_ok() {
            std::fs::write(&disk_path, b"this is not a jpeg").unwrap();
        }
        let data = get_image_base64_cached(&image_path, 100).unwrap();
        assert!(data.starts_with("data:image/jpeg;base64,"));
        let stored = std::fs::read(&disk_path).unwrap();
        assert!(stored.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn test_generate_thumbnail_jpeg() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_jpeg(temp_dir.path(), "source.jpg");
        let dst = temp_dir.path().join("thumb.jpg");
        generate_thumbnail(&src, &dst).unwrap();
        assert!(dst.exists());
        let thumb = image::open(&dst).unwrap();
        assert_eq!(thumb.width(), THUMBNAIL_SIZE as u32);
        assert_eq!(thumb.height(), THUMBNAIL_SIZE as u32);
    }

    #[test]
    fn test_generate_thumbnail_png() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_png(temp_dir.path(), "source.png");
        let dst = temp_dir.path().join("thumb.png");
        generate_thumbnail(&src, &dst).unwrap();
        assert!(dst.exists());
        let thumb = image::open(&dst).unwrap();
        assert_eq!(thumb.width(), THUMBNAIL_SIZE as u32);
        assert_eq!(thumb.height(), THUMBNAIL_SIZE as u32);
    }

    #[test]
    fn test_generate_thumbnail_raw() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_raw_with_jpeg_preview(temp_dir.path(), "photo.cr2");
        let dst = temp_dir.path().join("thumb.jpg");
        generate_thumbnail(&src, &dst).unwrap();
        assert!(dst.exists());
        let thumb = image::open(&dst).unwrap();
        assert_eq!(thumb.width(), THUMBNAIL_SIZE as u32);
        assert_eq!(thumb.height(), THUMBNAIL_SIZE as u32);
    }

    #[test]
    fn test_generate_thumbnail_raw_picks_largest() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_raw_with_multiple_jpegs(temp_dir.path(), "photo.nef");
        let dst = temp_dir.path().join("thumb.jpg");
        generate_thumbnail(&src, &dst).unwrap();
        assert!(dst.exists());
        let thumb = image::open(&dst).unwrap();
        assert_eq!(thumb.width(), THUMBNAIL_SIZE as u32);
        assert_eq!(thumb.height(), THUMBNAIL_SIZE as u32);
    }

    #[test]
    fn test_get_image_base64_jpeg() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_jpeg(temp_dir.path(), "photo.jpg");
        let result = get_image_base64(&src, 200).unwrap();
        assert!(result.starts_with("data:image/jpeg;base64,"));
        assert!(result.len() > 100);
    }

    #[test]
    fn test_get_image_base64_uses_cache() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_jpeg(temp_dir.path(), "cached.jpg");
        let result1 = get_image_base64(&src, THUMBNAIL_SIZE).unwrap();
        assert!(result1.starts_with("data:image/jpeg;base64,"));
        let thumb_path = get_thumbnail_path(&src).unwrap();
        assert!(thumb_path.exists());
        let result2 = get_image_base64(&src, THUMBNAIL_SIZE).unwrap();
        assert!(result2.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn test_get_image_base64_raw() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_raw_with_jpeg_preview(temp_dir.path(), "photo.nef");
        let result = get_image_base64(&src, 200).unwrap();
        assert!(result.starts_with("data:image/jpeg;base64,"));
        assert!(result.len() > 100);
    }

    #[test]
    fn test_unsupported_format_error() {
        let temp_dir = TempDir::new().unwrap();
        let fake = temp_dir.path().join("fake.xyz");
        std::fs::write(&fake, b"not an image").unwrap();
        let result = get_image_base64(&fake, 200);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unsupported image format") || err.contains("image::open failed"));
    }

    #[test]
    fn test_heic_format_gives_descriptive_error() {
        let temp_dir = TempDir::new().unwrap();
        let fake = temp_dir.path().join("photo.heic");
        std::fs::write(&fake, b"fake heic content").unwrap();
        let result = get_image_base64(&fake, 200);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("HEIC") || err.contains("heic") || err.contains("HEIF"));
    }

    #[test]
    fn test_hif_format_gives_descriptive_error() {
        let temp_dir = TempDir::new().unwrap();
        let fake = temp_dir.path().join("photo.hif");
        std::fs::write(&fake, b"fake hif content").unwrap();
        let result = get_image_base64(&fake, 200);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("HEIC") || err.contains("HEIF") || err.contains("hif"));
    }

    #[test]
    fn test_is_raw_extension() {
        assert!(is_raw_extension(Path::new("photo.CR2")));
        assert!(is_raw_extension(Path::new("photo.nef")));
        assert!(is_raw_extension(Path::new("photo.arw")));
        assert!(is_raw_extension(Path::new("photo.dng")));
        assert!(is_raw_extension(Path::new("photo.orf")));
        assert!(!is_raw_extension(Path::new("photo.jpg")));
        assert!(!is_raw_extension(Path::new("photo.png")));
    }

    #[test]
    fn test_is_heic_extension() {
        assert!(is_heic_extension(Path::new("photo.heic")));
        assert!(is_heic_extension(Path::new("photo.HEIF")));
        assert!(is_heic_extension(Path::new("photo.hif")));
        assert!(!is_heic_extension(Path::new("photo.jpg")));
        assert!(!is_heic_extension(Path::new("photo.cr2")));
    }

    #[test]
    fn test_jpeg_soi_detection() {
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let positions = find_all_jpeg_soi(&data);
        assert_eq!(positions, vec![0]);
        let data_no_soi = vec![0x00, 0x01, 0x02, 0x03];
        let positions = find_all_jpeg_soi(&data_no_soi);
        assert!(positions.is_empty());
    }

    #[test]
    fn test_jpeg_soi_detection_multiple() {
        let mut data = vec![0u8; 100];
        data[10] = 0xFF; data[11] = 0xD8;
        data[50] = 0xFF; data[51] = 0xD8;
        data[80] = 0xFF; data[81] = 0xD8;
        let positions = find_all_jpeg_soi(&data);
        assert_eq!(positions.len(), 3);
        assert!(positions.contains(&10));
        assert!(positions.contains(&50));
        assert!(positions.contains(&80));
    }

    #[test]
    fn test_jpeg_eoi_detection() {
        let data = vec![0xFF, 0xD8, 0xFF, 0xD9];
        assert_eq!(find_jpeg_eoi(&data, 2), Some(3));
        let data_no_eoi = vec![0xFF, 0xD8, 0x00, 0x00];
        assert_eq!(find_jpeg_eoi(&data_no_eoi, 2), None);
    }

    #[test]
    fn test_thumbnail_regenerated_after_source_change() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_jpeg(temp_dir.path(), "changing.jpg");
        let thumb1 = get_thumbnail_path(&src).unwrap();
        assert!(thumb1.exists());
        let new_img = image::RgbImage::from_pixel(200, 200, image::Rgb([50u8, 50, 50]));
        new_img.save(&src).unwrap();
        let thumb2 = get_thumbnail_path(&src).unwrap();
        let new_thumb = image::open(&thumb2).unwrap();
        assert_eq!(new_thumb.width(), THUMBNAIL_SIZE as u32);
    }

    #[test]
    fn test_open_image_propagates_error_for_bad_file() {
        let temp_dir = TempDir::new().unwrap();
        let bad = temp_dir.path().join("bad.jpg");
        std::fs::write(&bad, b"this is not a valid JPEG file at all").unwrap();
        let result = open_image(&bad);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("image::open failed") || err.contains("Unsupported"));
    }

    #[test]
    fn test_raw_without_jpeg_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let fake_raw = temp_dir.path().join("no_preview.cr2");
        let data = vec![0u8; 2048];
        std::fs::write(&fake_raw, &data).unwrap();
        let result = extract_raw_preview(&fake_raw);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("No embedded JPEG preview"));
    }
}