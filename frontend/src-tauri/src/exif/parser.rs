use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use exif::{In, Tag};

use super::thumbnail::{find_all_jpeg_soi, find_jpeg_eoi, is_raw_extension};
use super::ExifData;

/// Minimum embedded JPEG preview size to be considered usable for EXIF
/// extraction. Smaller previews are almost certainly spurious SOI markers
/// or thumbnail-sized and tend to lack the APP1 EXIF segment anyway.
const MIN_RAW_PREVIEW_BYTES: usize = 200;

/// Parse EXIF data from an image file
pub fn parse_exif<P: AsRef<Path>>(path: P) -> Result<ExifData, String> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    // RAW files (CR2/NEF/ARW/DNG/...) are TIFF-based containers that
    // kamadak-exif cannot read directly via `read_from_container`. Extract
    // EXIF from the largest embedded JPEG preview instead — this is where
    // camera firmware stores the full shooting EXIF for most RAW formats.
    if is_raw_extension(path) {
        return parse_raw_exif(path);
    }

    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut bufreader = BufReader::new(file);

    let exif_reader = exif::Reader::new();
    let exif = exif_reader
        .read_from_container(&mut bufreader)
        .map_err(|e| format!("Failed to read EXIF: {}", e))?;

    Ok(exif_to_data(&exif))
}

/// Parse EXIF from a RAW file by extracting its largest embedded JPEG preview
/// and reading EXIF from that JPEG via kamadak-exif.
///
/// Most camera RAW formats (CR2/NEF/ARW/DNG/...) embed a full-resolution
/// JPEG preview containing the complete shooting EXIF. This reuses the
/// SOI/EOI scanning logic shared with `thumbnail::extract_raw_preview` so
/// the same preview selection strategy applies to both paths.
fn parse_raw_exif(path: &Path) -> Result<ExifData, String> {
    let file_bytes =
        std::fs::read(path).map_err(|e| format!("Failed to read RAW file: {}", e))?;

    let soi_positions = find_all_jpeg_soi(&file_bytes);
    if soi_positions.is_empty() {
        return Err(
            "No embedded JPEG preview found in RAW file; EXIF extraction not supported"
                .to_string(),
        );
    }

    // Pick the largest complete JPEG segment — it is the most likely to
    // carry a full EXIF APP1 segment (smaller SOIs are often thumbnail
    // strips or spurious markers).
    let mut best_jpeg: Option<&[u8]> = None;
    let mut best_size: usize = 0;
    for &soi_pos in &soi_positions {
        if let Some(eoi_pos) = find_jpeg_eoi(&file_bytes, soi_pos.saturating_add(2)) {
            let jpeg = &file_bytes[soi_pos..=eoi_pos];
            if jpeg.len() > best_size {
                best_size = jpeg.len();
                best_jpeg = Some(jpeg);
            }
        }
    }

    let jpeg = best_jpeg.ok_or_else(|| {
        "No embedded JPEG preview found in RAW file (SOI without matching EOI); \
         EXIF extraction not supported"
            .to_string()
    })?;

    if jpeg.len() < MIN_RAW_PREVIEW_BYTES {
        return Err(format!(
            "Embedded JPEG preview too small ({} bytes < {}); RAW EXIF extraction not supported",
            jpeg.len(),
            MIN_RAW_PREVIEW_BYTES
        ));
    }

    let cursor = std::io::Cursor::new(jpeg);
    let mut bufreader = BufReader::new(cursor);
    let exif = exif::Reader::new()
        .read_from_container(&mut bufreader)
        .map_err(|e| format!("Failed to read EXIF from RAW preview: {}", e))?;

    Ok(exif_to_data(&exif))
}

/// Convert exif::Exif to our ExifData struct
fn exif_to_data(exif: &exif::Exif) -> ExifData {
    ExifData {
        make: get_string(exif, Tag::Make),
        model: get_string(exif, Tag::Model),
        lens_model: get_string(exif, Tag::LensModel),
        focal_length: get_rational(exif, Tag::FocalLength),
        aperture: get_rational(exif, Tag::FNumber),
        iso: get_u32(exif, Tag::ISOSpeed),
        exposure_time: get_rational(exif, Tag::ExposureTime),
        exposure_program: get_string(exif, Tag::ExposureProgram),
        metering_mode: get_string(exif, Tag::MeteringMode),
        flash: get_flash(exif),
        white_balance: get_string(exif, Tag::WhiteBalance),
        image_width: get_u32(exif, Tag::PixelXDimension)
            .or_else(|| get_u32(exif, Tag::ImageWidth)),
        image_height: get_u32(exif, Tag::PixelYDimension)
            .or_else(|| get_u32(exif, Tag::ImageLength)),
        datetime_original: get_string(exif, Tag::DateTimeOriginal),
        gps_latitude: get_gps_latitude(exif),
        gps_longitude: get_gps_longitude(exif),
    }
}

/// Get a string value from EXIF tag
fn get_string(exif: &exif::Exif, tag: Tag) -> Option<String> {
    exif.get_field(tag, In::PRIMARY).and_then(|field| {
        let s = field.display_value().to_string();
        if s.is_empty() {
            None
        } else {
            Some(s.trim().to_string())
        }
    })
}

/// Get a rational value from EXIF tag as f64
fn get_rational(exif: &exif::Exif, tag: Tag) -> Option<f64> {
    exif.get_field(tag, In::PRIMARY).and_then(|field| {
        let s = field.display_value().to_string();
        if s.is_empty() {
            return None;
        }

        if let Ok(v) = s.parse::<f64>() {
            return Some(v);
        }

        if let Some((num, den)) = s.split_once('/') {
            if let (Ok(n), Ok(d)) = (num.trim().parse::<f64>(), den.trim().parse::<f64>()) {
                if d != 0.0 {
                    return Some(n / d);
                }
            }
        }

        None
    })
}

/// Get a u32 value from EXIF tag
fn get_u32(exif: &exif::Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY).and_then(|field| {
        let s = field.display_value().to_string();
        if s.is_empty() {
            None
        } else {
            s.parse::<u32>().ok()
        }
    })
}

/// Get flash status from EXIF
fn get_flash(exif: &exif::Exif) -> Option<bool> {
    exif.get_field(Tag::Flash, In::PRIMARY).and_then(|field| {
        let s = field.display_value().to_string();
        if s.is_empty() {
            return None;
        }
        if let Ok(v) = s.parse::<u32>() {
            Some(v & 1 == 1)
        } else {
            None
        }
    })
}

/// Get GPS latitude in decimal degrees
fn get_gps_latitude(exif: &exif::Exif) -> Option<f64> {
    let lat = exif.get_field(Tag::GPSLatitude, In::PRIMARY)?;
    let lat_ref = exif.get_field(Tag::GPSLatitudeRef, In::PRIMARY)?;

    let lat_str = lat.display_value().to_string();
    let lat_ref_str = lat_ref.display_value().to_string();

    let parts: Vec<&str> = lat_str.split(", ").collect();
    if parts.len() < 3 {
        return None;
    }

    let deg: f64 = parts[0].trim().parse().ok()?;
    let min: f64 = parts[1].trim().parse().ok()?;
    let sec: f64 = parts[2].trim().parse().ok()?;

    let mut decimal = deg + min / 60.0 + sec / 3600.0;

    if lat_ref_str == "S" {
        decimal = -decimal;
    }

    Some(decimal)
}

/// Get GPS longitude in decimal degrees
fn get_gps_longitude(exif: &exif::Exif) -> Option<f64> {
    let lon = exif.get_field(Tag::GPSLongitude, In::PRIMARY)?;
    let lon_ref = exif.get_field(Tag::GPSLongitudeRef, In::PRIMARY)?;

    let lon_str = lon.display_value().to_string();
    let lon_ref_str = lon_ref.display_value().to_string();

    let parts: Vec<&str> = lon_str.split(", ").collect();
    if parts.len() < 3 {
        return None;
    }

    let deg: f64 = parts[0].trim().parse().ok()?;
    let min: f64 = parts[1].trim().parse().ok()?;
    let sec: f64 = parts[2].trim().parse().ok()?;

    let mut decimal = deg + min / 60.0 + sec / 3600.0;

    if lon_ref_str == "W" {
        decimal = -decimal;
    }

    Some(decimal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;
    use tempfile::TempDir;

    #[test]
    fn test_parse_exif_file_not_found() {
        let result = parse_exif("nonexistent.jpg");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("File not found"));
    }

    #[test]
    fn test_parse_exif_invalid_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"This is not a JPEG file").unwrap();

        let result = parse_exif(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_exif_no_exif_data() {
        // Minimal valid JPEG without EXIF (just SOI + EOI)
        let jpeg = vec![0xFF, 0xD8, 0xFF, 0xD9];
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(&jpeg).unwrap();

        let result = parse_exif(temp_file.path());
        assert!(result.is_err());
    }

    // ---- RAW EXIF parsing tests (tasks 3.2/3.3) ----

    /// Build a fake RAW file with an embedded JPEG preview (SOI..EOI).
    fn create_raw_with_embedded_jpeg(dir: &Path, name: &str, jpeg_bytes: &[u8]) -> PathBuf {
        let path = dir.join(name);
        // Lead with padding that mimics a RAW container header; the parser
        // must scan past it to find the JPEG SOI marker.
        let mut data = vec![0u8; 512];
        data.extend_from_slice(jpeg_bytes);
        std::fs::write(&path, &data).unwrap();
        path
    }

    fn minimal_jpeg_no_exif() -> Vec<u8> {
        // SOI + EOI — valid JPEG structure but no EXIF APP1 segment.
        vec![0xFF, 0xD8, 0xFF, 0xD9]
    }

    #[test]
    fn test_parse_raw_routes_by_extension() {
        // A .cr2 file with an embedded JPEG must be routed through the RAW
        // parser path (not the plain kamadak-exif container reader).
        let temp_dir = TempDir::new().unwrap();
        let raw_path =
            create_raw_with_embedded_jpeg(temp_dir.path(), "photo.cr2", &minimal_jpeg_no_exif());
        let result = parse_exif(&raw_path);
        assert!(result.is_err(), "RAW with no-EXIF JPEG should error");
        // Error must come from the RAW path, mentioning the preview/EXIF step.
        let err = result.unwrap_err();
        assert!(
            err.contains("RAW") || err.contains("EXIF"),
            "expected RAW-path error, got: {err}"
        );
    }

    #[test]
    fn test_parse_raw_no_embedded_jpeg_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let raw_path = temp_dir.path().join("photo.nef");
        // Bytes with no JPEG SOI marker anywhere.
        std::fs::write(&raw_path, vec![0u8; 2048]).unwrap();

        let result = parse_exif(&raw_path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("No embedded JPEG"),
            "expected 'No embedded JPEG' error, got: {err}"
        );
    }

    #[test]
    fn test_parse_raw_preview_too_small_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        // Embedded JPEG is valid structurally but well under the 200-byte
        // threshold the parser uses to reject unusable previews.
        let raw_path =
            create_raw_with_embedded_jpeg(temp_dir.path(), "photo.arw", &minimal_jpeg_no_exif());
        let result = parse_exif(&raw_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_raw_picks_largest_embedded_jpeg() {
        // Two embedded JPEGs — only the larger one is structurally valid
        // enough to be worth probing; the parser must not fail just
        // because a smaller spurious SOI appears earlier in the stream.
        let temp_dir = TempDir::new().unwrap();
        let small = minimal_jpeg_no_exif(); // 4 bytes
        let mut large = vec![0xFF, 0xD8]; // SOI
        large.extend(vec![0u8; 300]); // body
        large.push(0xFF);
        large.push(0xD9); // EOI — >200 bytes total
        let mut combined = Vec::new();
        combined.extend_from_slice(&small);
        combined.extend_from_slice(&large);
        let raw_path = temp_dir.path().join("photo.rw2");
        let mut data = vec![0u8; 256];
        data.extend_from_slice(&combined);
        std::fs::write(&raw_path, &data).unwrap();

        let result = parse_exif(&raw_path);
        assert!(result.is_err());
        // Routed through RAW path (the large preview has no EXIF, so it
        // fails at EXIF read, not at "no preview").
        let err = result.unwrap_err();
        assert!(
            err.contains("RAW") || err.contains("EXIF"),
            "expected RAW-path error, got: {err}"
        );
    }

    #[test]
    fn test_parse_raw_dng_extension_supported() {
        let temp_dir = TempDir::new().unwrap();
        let raw_path = temp_dir.path().join("photo.dng");
        std::fs::write(&raw_path, vec![0u8; 1024]).unwrap();
        let result = parse_exif(&raw_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No embedded JPEG"));
    }
}
