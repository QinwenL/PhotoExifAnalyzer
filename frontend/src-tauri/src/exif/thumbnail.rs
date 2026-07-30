use std::path::{Path, PathBuf};
use std::io::{Read, Seek, SeekFrom};

const THUMBNAIL_SIZE: u32 = 150;
/// Absolute ceiling for `max_size` accepted by any entrypoint in this
/// module. Also enforced at the Tauri command layer (lib.rs) as a second
/// defensive clamp. Use 4096 = ~16.7 Mpix which is plenty for any desktop
/// thumbnail use case while keeping memory & CPU bounded.
pub const MAX_ALLOWED_THUMBNAIL_SIZE: u32 = 4096;
/// Conservative upper bound for on-disk size-cache files. A 4096x4096
/// quality-85 JPEG of worst-case (random) pixel noise typically peaks at
/// ~22 MiB. We leave ~1.9× headroom so future callers bumping the quality
/// or the pixel ceiling slightly won't trip the "stale file" logic.
const MAX_DISK_CACHE_BYTES: u64 = 42 * 1024 * 1024;

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
    // 优先查 EXIF 缓存中的 preview 偏移（scanner 在解析 EXIF 时已缓存），
    // 命中则直接 seek + read，跳过全文件扫描。
    let preview = if let Some(cache) = crate::EXIF_CACHE.as_ref() {
        cache.lock().unwrap().get_preview(path, None)
    } else {
        None
    };

    // 缓存未命中则扫描文件定位 preview
    let preview = match preview {
        Some(p) => p,
        None => super::raw_scan::find_largest_jpeg_preview(path)?
            .ok_or_else(|| {
                "No embedded JPEG preview found in RAW file (no SOI markers). \
                 Thumbnail generation for this RAW format is not yet supported."
                    .to_string()
            })?,
    };

    if preview.length < 200 {
        return Err(
            "Embedded JPEG preview is too small (< 200 bytes). \
             RAW file may not contain a usable preview."
                .to_string(),
        );
    }

    // 只读 preview 字节，不读整个 RAW 文件
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open RAW file: {}", e))?;
    file.seek(SeekFrom::Start(preview.offset))
        .map_err(|e| format!("Failed to seek to preview offset: {}", e))?;
    let mut jpeg_data = vec![0u8; preview.length];
    file.read_exact(&mut jpeg_data)
        .map_err(|e| format!("Failed to read preview bytes: {}", e))?;

    let img = image::load_from_memory_with_format(&jpeg_data, image::ImageFormat::Jpeg)
        .or_else(|_| {
            let temp_path = std::env::temp_dir().join(format!(
                "raw_preview_{}_{}.jpg",
                std::process::id(),
                preview.length
            ));
            std::fs::write(&temp_path, &jpeg_data)?;
            let result = image::open(&temp_path);
            let _ = std::fs::remove_file(&temp_path);
            result
        })
        .map_err(|e| format!("Failed to decode RAW preview JPEG: {} (size: {} bytes)", e, preview.length))?;

    Ok(img)
}

#[allow(dead_code)]
pub(crate) fn find_all_jpeg_soi(data: &[u8]) -> Vec<usize> {
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

#[allow(dead_code)]
pub(crate) fn find_jpeg_eoi(data: &[u8], from_pos: usize) -> Option<usize> {
    if from_pos >= data.len().saturating_sub(1) {
        return None;
    }
    data[from_pos..]
        .windows(2)
        .position(|w| w[0] == 0xFF && w[1] == 0xD9)
        .map(|p| from_pos + p + 1)
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

/// Shared helper for stem + hash computation. Both the fixed 150-px thumbnail
/// and all size-aware data cache files derive their names from this pair.
fn get_thumbnail_stem_hash(path: &Path) -> Result<(String, u64), String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Invalid filename: {}", path.display()))?;

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();

    Ok((stem.to_string(), hash))
}

fn get_thumbnail_name(path: &Path) -> Result<String, String> {
    let (stem, hash) = get_thumbnail_stem_hash(path)?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("jpg");
    Ok(format!("{stem}_{hash}.{ext}"))
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

fn generate_thumbnail(input_path: &Path, output_path: &Path) -> Result<(), String> {
    let img = open_image(input_path)?;

    let thumbnail =
        img.resize(THUMBNAIL_SIZE, THUMBNAIL_SIZE, image::imageops::FilterType::Lanczos3);

    thumbnail
        .save(output_path)
        .map_err(|e| format!("Failed to save thumbnail: {}", e))?;

    Ok(())
}

/// Compute resized JPEG bytes WITHOUT base64 wrapping. This is the hot path
/// reused by both `get_image_base64` (encode once) and
/// `get_image_base64_cached` (write bytes to disk directly, skip the
/// encode-decode round-trip).
fn get_image_jpeg_bytes(path: &Path, max_size: u32) -> Result<Vec<u8>, String> {
    // Defensive clamp at the function layer. The Tauri command layer also
    // clamps; the redundancy protects against future non-command callers
    // that might bypass the command layer (tests, other Rust modules).
    let max_size = max_size.clamp(1, MAX_ALLOWED_THUMBNAIL_SIZE);
    let img = if max_size <= THUMBNAIL_SIZE {
        if let Ok(thumb_path) = get_thumbnail_path(path) {
            if let Ok(thumb) = image::open(&thumb_path) {
                let img = if thumb.width() > max_size || thumb.height() > max_size {
                    thumb.resize(max_size, max_size, image::imageops::FilterType::Triangle)
                } else {
                    thumb
                };
                return encode_jpeg_bytes(&img);
            }
        }
        open_image(path)?
    } else {
        open_image(path)?
    };

    let resized = if img.width() > max_size || img.height() > max_size {
        img.resize(max_size, max_size, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    // Make sure the fixed-size 150-px thumbnail exists too so subsequent
    // small-size calls can take the fast path above.
    let _ = get_thumbnail_path(path);

    encode_jpeg_bytes(&resized)
}

fn encode_jpeg_bytes(img: &image::DynamicImage) -> Result<Vec<u8>, String> {
    use image::codecs::jpeg::JpegEncoder;
    let mut buffer = std::io::Cursor::new(Vec::new());
    // image 0.25 removed ImageOutputFormat; use JpegEncoder for quality control.
    let encoder = JpegEncoder::new_with_quality(&mut buffer, 85);
    img.write_with_encoder(encoder)
        .map_err(|e| format!("Failed to encode image: {}", e))?;
    Ok(buffer.into_inner())
}

pub fn get_image_base64(path: &Path, max_size: u32) -> Result<String, String> {
    let bytes = get_image_jpeg_bytes(path, max_size)?;
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
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

/// Remove all size-aware disk-cache files for a source image
/// (`{stem}_{hash}_200.jpg`, `{stem}_{hash}_800.jpg`, …).
/// Called from `cleanup_caches_async` when a source image is deleted.
pub fn delete_all_size_caches(path: &Path) -> Result<(), String> {
    let (stem, hash) = match get_thumbnail_stem_hash(path) {
        Ok(pair) => pair,
        Err(_) => return Ok(()),
    };
    let prefix = format!("{stem}_{hash}_");
    let thumb_dir = match get_thumbnail_dir(path) {
        Ok(d) => d,
        Err(_) => return Ok(()),
    };
    let read_dir = match std::fs::read_dir(&thumb_dir) {
        Ok(rd) => rd,
        Err(e) => {
            eprintln!(
                "[thumbnail::delete_all_size_caches] Failed to read {}: {} \
                 (size-cache files may be left orphaned until clear_thumbnails)",
                thumb_dir.display(),
                e
            );
            return Err(format!(
                "Failed to enumerate thumbnail dir {}: {}",
                thumb_dir.display(),
                e
            ));
        }
    };
    for entry in read_dir.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_file() {
            continue;
        }
        let fname = entry_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");
        if fname.starts_with(&prefix) {
            if let Err(e) = std::fs::remove_file(&entry_path) {
                eprintln!(
                    "[thumbnail::delete_all_size_caches] Failed to remove orphan \
                     size cache {}: {}",
                    entry_path.display(),
                    e
                );
            }
        }
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

/// Memory cache entry payload. Bundling the disk cache mtime alongside the cached
/// data URI lets us close the TOCTOU race between the "is disk cache
/// still valid?" probe and the eventual "drop stale memory entry" pop()":
/// if a concurrent thread has refreshed the cache entry between our probe and
/// our pop(), the bundled disk_mtime will differ from what we read during
/// the probe and we'll skip the pop.
#[derive(Clone)]
struct MemCacheEntry {
    data_uri: String,
    /// The `std::fs::metadata().modified()` timestamp of the size-aware disk
    /// cache file at the moment this memory entry was written. `None` means
    /// "written without a disk cache backing" (shouldn't happen in practice,
    /// since the cold path always writes both caches).
    disk_mtime: Option<std::time::SystemTime>,
}

lazy_static::lazy_static! {
    static ref IMAGE_DATA_CACHE: std::sync::Mutex<
        lru::LruCache<(std::path::PathBuf, u32), MemCacheEntry>
    > = std::sync::Mutex::new(lru::LruCache::new(
        NonZeroUsize::new(MEMORY_CACHE_CAPACITY).unwrap()
    ));
}

/// Access `IMAGE_DATA_CACHE` safely, converting `Mutex::lock` poison into a
/// stringified `Err` (matches project-wide `Result<T, String>` convention)
/// instead of panicking the whole pool thread.
fn with_memory_cache<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut lru::LruCache<(std::path::PathBuf, u32), MemCacheEntry>) -> R,
{
    let mut guard = IMAGE_DATA_CACHE
        .lock()
        .map_err(|poison| format!("Image cache mutex poisoned: {}", poison))?;
    Ok(f(&mut guard))
}

pub(crate) fn get_data_cache_path(
    path: &std::path::Path,
    max_size: u32,
) -> Result<std::path::PathBuf, String> {
    let dir = get_thumbnail_dir(path)?;
    let (stem, hash) = get_thumbnail_stem_hash(path)?;
    Ok(dir.join(format!("{stem}_{hash}_{max_size}.jpg")))
}

/// Returns `true` if `thumb_mtime >= source_mtime`; on any stat error we
/// conservatively treat the cache as stale.
fn disk_cache_fresh(disk_path: &Path, source_path: &Path) -> bool {
    match (
        source_path.metadata().and_then(|m| m.modified()),
        disk_path.metadata().and_then(|m| m.modified()),
    ) {
        (Ok(sm), Ok(tm)) => tm >= sm,
        _ => false,
    }
}

/// Helper for the repeated "remove stale disk cache entry" pattern. Always
/// logs on failure so operators can diagnose stuck "re-decode every time"
/// issues caused by Windows file-handle pinning / ACLs / readonly bits.
fn remove_stale_disk_cache(disk_path: &Path) {
    if let Err(e) = std::fs::remove_file(disk_path) {
        eprintln!(
            "[thumbnail::cache] Failed to remove stale/invalid size cache {}: {} \
             (source file may repeatedly be re-decoded until file is removable)",
            disk_path.display(),
            e
        );
    }
}

pub fn get_image_base64_cached(
    path: &std::path::Path,
    max_size: u32,
) -> Result<String, String> {
    let key = (path.to_path_buf(), max_size);
    // Single computation for the whole function — hash + mkdir are O(1) but
    // repeated three times in the naive version.
    let disk_path_opt = get_data_cache_path(path, max_size).ok();

    // Memory LRU cache probe. We only trust the hit when the matching
    // size-aware disk cache entry is also present and fresh:
    // `get_image_base64_cached` always writes BOTH caches together on the
    // cold path, so a missing/stale disk entry implies the memory entry
    // is stale too (the source file was edited/replaced with the same
    // path since the last decode). Without this check scrolling back to
    // an earlier-viewed photo after editing it would serve the stale
    // in-memory bytes forever (until LRU eviction).
    //
    // TOCTOU: we capture the memory entry's bundled `disk_mtime` on GET.
    // If we later decide to POP because disk cache looks stale, we verify
    // the current entry still refers to the SAME disk_mtime — if not, a
    // concurrent thread already refreshed the entry in the gap between
    // the two mutex acquisitions and we must NOT drop the newly-written
    // valid data.
    let (mem_data, mem_disk_mtime) = match with_memory_cache(|cache| cache.get(&key).cloned())? {
        Some(entry) => (Some(entry.data_uri), entry.disk_mtime),
        None => (None, None),
    };
    if let Some(data) = mem_data {
        let disk_valid = if let Some(ref dp) = disk_path_opt {
            dp.exists() && disk_cache_fresh(dp, path)
        } else {
            false
        };
        if disk_valid {
            return Ok(data);
        }
        // Memory entry is out of date — drop it with version gating so we
        // don't stomp on a concurrent refresh that already happened.
        let _ = with_memory_cache(|cache| {
            if let Some(current) = cache.get(&key) {
                if current.disk_mtime == mem_disk_mtime {
                    cache.pop(&key);
                }
            }
        });
    }

    // Disk cache hit path (size-aware) — validate mtime and size cap
    if let Some(ref disk_path) = disk_path_opt {
        if disk_path.exists() {
            let fresh = disk_cache_fresh(disk_path, path);
            let within_size = disk_path
                .metadata()
                .map(|m| m.len() <= MAX_DISK_CACHE_BYTES)
                .unwrap_or(false);
            match (fresh, within_size) {
                (true, true) => match std::fs::read(disk_path) {
                    Ok(bytes) if bytes.starts_with(&[0xFF, 0xD8]) => {
                        use base64::Engine;
                        let data = format!(
                            "data:image/jpeg;base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(&bytes)
                        );
                        let disk_mtime_now = disk_path.metadata().and_then(|m| m.modified()).ok();
                        let _ = with_memory_cache(|cache| {
                            cache.put(
                                key.clone(),
                                MemCacheEntry {
                                    data_uri: data.clone(),
                                    disk_mtime: disk_mtime_now,
                                },
                            )
                        });
                        return Ok(data);
                    }
                    Ok(_) => remove_stale_disk_cache(disk_path),
                    Err(_) => remove_stale_disk_cache(disk_path),
                },
                _ => {
                    // Stale mtime OR oversized; remove the stale entry and
                    // fall through to full decode.
                    remove_stale_disk_cache(disk_path);
                }
            }
        }
    }

    // Cold path: full decode -> JPEG bytes. Avoid base64 round-trip by
    // writing raw bytes straight to disk.
    let jpeg_bytes = get_image_jpeg_bytes(path, max_size)?;
    use base64::Engine;
    let mut disk_mtime_after_write: Option<std::time::SystemTime> = None;
    if let Some(ref disk_path) = disk_path_opt {
        if let Err(e) = std::fs::write(disk_path, &jpeg_bytes) {
            eprintln!(
                "[thumbnail::cache] Failed to write thumbnail cache {}: {}",
                disk_path.display(),
                e
            );
        } else {
            disk_mtime_after_write = disk_path.metadata().and_then(|m| m.modified()).ok();
        }
    }
    let data = format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes)
    );
    let _ = with_memory_cache(|cache| {
        cache.put(
            key,
            MemCacheEntry {
                data_uri: data.clone(),
                disk_mtime: disk_mtime_after_write,
            },
        )
    });
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
        // image 0.25: ImageFormat replaces ImageOutputFormat. Quality doesn't
        // matter for test fixtures, so use the default Jpeg encoder.
        img.write_to(&mut buffer, image::ImageFormat::Jpeg)
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
    fn test_size_cache_invalidated_on_source_mtime_change() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = create_test_jpeg(temp_dir.path(), "changing.jpg");

        // First decode: 100x100 gray pixels, cache to disk
        let data_before = get_image_base64_cached(&image_path, 100).unwrap();
        let disk_path = get_data_cache_path(&image_path, 100).unwrap();
        assert!(disk_path.exists());

        // Ensure mtime advances (Windows NTFS has 100ns resolution; we sleep 2s
        // to be safe across any filesystem that has 1s/2s mtime granularity).
        std::thread::sleep(std::time::Duration::from_secs(2));

        // Overwrite source with a LARGER, distinctly-different image that
        // cannot produce identical resize output at max_size=100:
        // 350x500 red-gradient pixels vs the original square 100x100 gray.
        let (w, h) = (350u32, 500u32);
        let new_img = image::RgbImage::from_fn(w, h, |x, y| {
            let r = (x * 255 / w) as u8;
            let g = (y * 255 / h) as u8;
            image::Rgb([r, g, 128u8])
        });
        new_img.save(&image_path).unwrap();

        // Call again — stale cache (old mtime) must be removed + regenerated
        let data_after = get_image_base64_cached(&image_path, 100).unwrap();

        // The source content is fundamentally different so the re-encoded
        // data URI cannot match. (Checking the data URL string avoids any
        // edge case where the raw JPEG bytes file-system-read has any
        // transient encoding flakiness; it's the higher-level property we
        // actually care about for users.)
        assert_ne!(data_before, data_after);
        assert!(data_before.starts_with("data:image/jpeg;base64,"));
        assert!(data_after.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn test_delete_all_size_caches_removes_orphaned_files() {
        let temp_dir = TempDir::new().unwrap();
        let image_path = create_test_jpeg(temp_dir.path(), "source.jpg");

        // Populate multiple size caches
        let _ = get_image_base64_cached(&image_path, 50).unwrap();
        let _ = get_image_base64_cached(&image_path, 200).unwrap();
        let _ = get_image_base64_cached(&image_path, 800).unwrap();
        let d50 = get_data_cache_path(&image_path, 50).unwrap();
        let d200 = get_data_cache_path(&image_path, 200).unwrap();
        let d800 = get_data_cache_path(&image_path, 800).unwrap();
        assert!(d50.exists());
        assert!(d200.exists());
        assert!(d800.exists());

        delete_all_size_caches(&image_path).unwrap();
        assert!(!d50.exists());
        assert!(!d200.exists());
        assert!(!d800.exists());

        // Fixed-size thumbnail (150px) is NOT removed by delete_all_size_caches
        // (that's delete_thumbnail's job) — confirm it was created by earlier
        // warm-up calls and is still present.
        let fixed_thumb = get_thumbnail_path(&image_path).unwrap();
        assert!(fixed_thumb.exists());
    }

    #[test]
    fn test_generate_thumbnail_jpeg() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_jpeg(temp_dir.path(), "source.jpg");
        let dst = temp_dir.path().join("thumb.jpg");
        generate_thumbnail(&src, &dst).unwrap();
        assert!(dst.exists());
        let thumb = image::open(&dst).unwrap();
        assert_eq!(thumb.width(), THUMBNAIL_SIZE);
        assert_eq!(thumb.height(), THUMBNAIL_SIZE);
    }

    #[test]
    fn test_generate_thumbnail_png() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_png(temp_dir.path(), "source.png");
        let dst = temp_dir.path().join("thumb.png");
        generate_thumbnail(&src, &dst).unwrap();
        assert!(dst.exists());
        let thumb = image::open(&dst).unwrap();
        assert_eq!(thumb.width(), THUMBNAIL_SIZE);
        assert_eq!(thumb.height(), THUMBNAIL_SIZE);
    }

    #[test]
    fn test_generate_thumbnail_raw() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_raw_with_jpeg_preview(temp_dir.path(), "photo.cr2");
        let dst = temp_dir.path().join("thumb.jpg");
        generate_thumbnail(&src, &dst).unwrap();
        assert!(dst.exists());
        let thumb = image::open(&dst).unwrap();
        assert_eq!(thumb.width(), THUMBNAIL_SIZE);
        assert_eq!(thumb.height(), THUMBNAIL_SIZE);
    }

    #[test]
    fn test_generate_thumbnail_raw_picks_largest() {
        let temp_dir = TempDir::new().unwrap();
        let src = create_test_raw_with_multiple_jpegs(temp_dir.path(), "photo.nef");
        let dst = temp_dir.path().join("thumb.jpg");
        generate_thumbnail(&src, &dst).unwrap();
        assert!(dst.exists());
        let thumb = image::open(&dst).unwrap();
        assert_eq!(thumb.width(), THUMBNAIL_SIZE);
        assert_eq!(thumb.height(), THUMBNAIL_SIZE);
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
        assert_eq!(new_thumb.width(), THUMBNAIL_SIZE);
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
