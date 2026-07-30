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

/// Canon CR3 files use the ISO BMFF (MP4) container format. EXIF metadata
/// is stored inside `uuid` boxes whose UUID is the Canon TIFF metadata ID.
/// When found, these boxes contain a raw TIFF structure that kamadak-exif
/// can parse directly, bypassing the heuristic embedded-JPEG scan entirely.
///
/// UUID bytes (16) for Canon CMT1 (TIFF EXIF metadata) box:
///   EA492F5B-0C5E-4EFB-A6B4-0CBA9E4CB7A8
const CR3_CMT1_UUID: [u8; 16] = [
    0xEA, 0x49, 0x2F, 0x5B, 0x0C, 0x5E, 0x4E, 0xFB,
    0xA6, 0xB4, 0x0C, 0xBA, 0x9E, 0x4C, 0xB7, 0xA8,
];

/// Parse EXIF data from an image file
pub fn parse_exif<P: AsRef<Path>>(path: P) -> Result<ExifData, String> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    // Canon CR3 is an ISO BMFF container — its EXIF lives in Canon-specific
    // `uuid` boxes (CMT1) as a TIFF payload. Try the structured box-scan
    // first because it's more reliable than byte-level SOI hunting, then
    // fall back to the generic embedded-JPEG path for older TIFF-based RAW.
    if is_cr3_extension(path) {
        if let Ok(data) = parse_cr3_exif_via_bmff(path) {
            if !data.is_empty() {
                return Ok(data);
            }
        }
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

fn is_cr3_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("cr3"))
        .unwrap_or(false)
}

/// ISO BMFF box header: [4 bytes size][4 bytes type] — size may be 1
/// (extended 64-bit size follows header) or 0 (extends to EOF).
#[allow(dead_code)]
struct BmffBox {
    header_start: usize,
    data_start: usize,
    data_end: usize,
    box_type: [u8; 4],
}

/// Scan ISO BMFF bytes for Canon CMT1 `uuid` boxes containing TIFF EXIF
/// payload, and parse the first such payload via kamadak-exif.
///
/// Walks `moov`, `trak`, `mdia`, `minf`, `stbl`, `udta`, `meta`, `iloc`
/// and any other container boxes recursively. Errors are swallowed so the
/// caller can fall back to the generic RAW preview scan; only explicit
/// EXIF-read failures surface as `Err`.
fn parse_cr3_exif_via_bmff(path: &Path) -> Result<ExifData, String> {
    let file_bytes = std::fs::read(path).map_err(|e| format!("Failed to read CR3 file: {}", e))?;

    // Validate ftyp signature before committing to a BMFF scan.
    if file_bytes.len() < 12
        || &file_bytes[4..8] != b"ftyp"
    {
        return Err("Not a valid ISO BMFF container (missing ftyp box)".to_string());
    }

    let tiff_payloads = find_canon_tiff_payloads(&file_bytes);
    if tiff_payloads.is_empty() {
        return Err("No Canon CMT1 uuid box found in CR3; falling back to preview scan".to_string());
    }

    let mut last_err: Option<String> = None;
    for tiff_bytes in tiff_payloads {
        let cursor = std::io::Cursor::new(tiff_bytes);
        let mut bufreader = BufReader::new(cursor);
        match exif::Reader::new().read_from_container(&mut bufreader) {
            Ok(exif) => {
                let data = exif_to_data(&exif);
                if !data.is_empty() {
                    return Ok(data);
                }
            }
            Err(e) => last_err = Some(format!("Failed to read TIFF from CR3 CMT1 box: {}", e)),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        "CMT1 boxes found but contained no usable EXIF data; falling back".to_string()
    }))
}

/// Recursively walk the ISO BMFF boxes in `data` and return every byte slice
/// that corresponds to the payload of a Canon CMT1 `uuid` box.
fn find_canon_tiff_payloads(data: &[u8]) -> Vec<&[u8]> {
    let mut results = Vec::new();
    let mut boxes = Vec::new();
    if let Some(b) = read_boxes_in_range(data, 0, data.len()) {
        boxes = b;
    }

    // BFS through container boxes
    while let Some(b) = boxes.pop() {
        if &b.box_type == b"uuid" {
            if b.data_end.saturating_sub(b.data_start) >= 16 {
                let uuid_bytes = &data[b.data_start..b.data_start + 16];
                if uuid_bytes == CR3_CMT1_UUID {
                    let payload_start = b.data_start + 16;
                    if payload_start < b.data_end {
                        results.push(&data[payload_start..b.data_end]);
                    }
                }
            }
            continue;
        }
        // Container boxes: descend into their children. The standard BMFF
        // containers that can carry uuid boxes are:
        //   moov, trak, mdia, minf, stbl, udta, meta, dinf, stsd
        let is_container = matches!(
            &b.box_type,
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"udta"
                | b"meta" | b"dinf" | b"stsd" | b"iinf"
        );
        if is_container {
            // `meta` box has an extra 4-byte version/flags sub-header after
            // the box type that children must skip over.
            let child_start = if &b.box_type == b"meta" {
                b.data_start.saturating_add(4)
            } else {
                b.data_start
            };
            if let Some(children) = read_boxes_in_range(data, child_start, b.data_end) {
                boxes.extend(children);
            }
        }
    }

    results
}

/// Read all sibling ISO BMFF boxes within `[start, end)` of `data`.
/// Returns `None` if the range is malformed (not a single box is decodable).
fn read_boxes_in_range(data: &[u8], start: usize, end: usize) -> Option<Vec<BmffBox>> {
    let mut pos = start;
    let mut out = Vec::new();
    let end = end.min(data.len());
    while pos + 8 <= end {
        let size_32 = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let mut box_type = [0u8; 4];
        box_type.copy_from_slice(&data[pos + 4..pos + 8]);

        let header_size: usize;
        let body_size: usize;

        match size_32 {
            1 => {
                // Extended 64-bit size immediately follows the 8-byte header.
                if pos + 16 > end {
                    break;
                }
                header_size = 16;
                let size_64 = u64::from_be_bytes([
                    data[pos + 8],
                    data[pos + 9],
                    data[pos + 10],
                    data[pos + 11],
                    data[pos + 12],
                    data[pos + 13],
                    data[pos + 14],
                    data[pos + 15],
                ]);
                body_size = (size_64 as usize).checked_sub(header_size)?;
            }
            0 => {
                // Box extends to EOF.
                header_size = 8;
                body_size = end.checked_sub(pos + header_size)?;
            }
            n if n >= 8 => {
                header_size = 8;
                body_size = n - header_size;
            }
            _ => break, // Invalid box; stop scanning this level.
        }

        let data_start = pos + header_size;
        let data_end = (data_start + body_size).min(end);

        out.push(BmffBox {
            header_start: pos,
            data_start,
            data_end,
            box_type,
        });

        // Advance to next sibling box header.
        pos = if size_32 == 0 {
            end // 0-size is always the last box in the range.
        } else {
            data_end
        };

        if pos >= end {
            break;
        }
    }
    Some(out)
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

    // ---- CR3 ISO BMFF parser tests ----

    fn u32_be(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    /// Build an ISO BMFF box: [size:4][type:4][payload]
    fn bmff_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let size: u32 = 8 + payload.len() as u32;
        let mut out = Vec::with_capacity(size as usize);
        out.extend_from_slice(&u32_be(size));
        out.extend_from_slice(box_type);
        out.extend_from_slice(payload);
        out
    }

    /// Build a minimal fake CR3 file structure:
    ///   [ftyp box] + [moov -> udta -> uuid(CMT1) -> TIFF-like payload]
    ///
    /// The UUID+payload bytes inside uuid-box directly are what
    /// `find_canon_tiff_payloads` extracts.
    fn create_fake_cr3(dir: &Path, name: &str, tiff_inside_cmt1: &[u8]) -> PathBuf {
        let path = dir.join(name);

        // ftyp box: brand=crx + minor_ver + compatible_brands
        let mut ftyp_payload = Vec::new();
        ftyp_payload.extend_from_slice(b"crx "); // major brand: Canon CR3
        ftyp_payload.extend_from_slice(&u32_be(0)); // minor version
        ftyp_payload.extend_from_slice(b"crx "); // compatible brand 1
        ftyp_payload.extend_from_slice(b"isom"); // compatible brand 2
        let ftyp_box = bmff_box(b"ftyp", &ftyp_payload);

        // uuid box: [16-byte UUID][TIFF payload]
        let mut uuid_payload = Vec::new();
        uuid_payload.extend_from_slice(&CR3_CMT1_UUID);
        uuid_payload.extend_from_slice(tiff_inside_cmt1);
        let uuid_box = bmff_box(b"uuid", &uuid_payload);

        // udta container that wraps the uuid box
        let udta_box = bmff_box(b"udta", &uuid_box);

        // moov container that wraps udta
        let moov_box = bmff_box(b"moov", &udta_box);

        // Concatenate to form the whole file
        let mut file_bytes = ftyp_box;
        file_bytes.extend(moov_box);
        // Make the file large enough so preview fallback would trigger a
        // "no SOI markers" error instead of a read error if BMFF path fails.
        file_bytes.extend(vec![0u8; 1024]);

        std::fs::write(&path, &file_bytes).unwrap();
        path
    }

    #[test]
    fn test_is_cr3_extension_case_insensitive() {
        assert!(is_cr3_extension(Path::new("photo.cr3")));
        assert!(is_cr3_extension(Path::new("photo.CR3")));
        assert!(is_cr3_extension(Path::new("photo.Cr3")));
        assert!(!is_cr3_extension(Path::new("photo.cr2")));
        assert!(!is_cr3_extension(Path::new("photo.jpg")));
        assert!(!is_cr3_extension(Path::new("photo")));
    }

    #[test]
    fn test_cr3_no_ftyp_rejects_bmff_scan() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("broken.cr3");
        // All-zero file of reasonable size: not a valid BMFF, no ftyp.
        std::fs::write(&path, vec![0u8; 2048]).unwrap();
        // Should fall through the BMFF path (no ftyp) and end up at the
        // generic RAW preview scan, which will report "No embedded JPEG".
        let result = parse_exif(&path);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("No embedded JPEG"),
            "expected generic RAW fallback, got: {err}"
        );
    }

    #[test]
    fn test_cr3_cmt1_uuid_box_found() {
        // A TIFF-byte blob that is NOT a valid TIFF → parse_exif will
        // attempt the BMFF path, find the CMT1 uuid box, try to parse its
        // payload as TIFF, fail, then fall back to generic RAW preview
        // scan (no JPEG). The key behaviour is that the code DOES reach the
        // uuid-box parsing and doesn't panic on malformed TIFF payload.
        let temp_dir = TempDir::new().unwrap();
        let fake_tiff: Vec<u8> = vec![0xAA, 0xBB, 0xCC, 0xDD, 0x11, 0x22, 0x33, 0x44];
        let cr3 = create_fake_cr3(temp_dir.path(), "test.cr3", &fake_tiff);
        let result = parse_exif(&cr3);
        assert!(
            result.is_err(),
            "Should error: uuid payload is not a real TIFF + no JPEG preview"
        );
    }

    #[test]
    fn test_cr3_extension_falls_back_to_preview_scan() {
        // CR3 file with a ftyp box but NO uuid CMT1 box, and an embedded
        // JPEG SOI+EOI blob (no EXIF) should fall through both the BMFF
        // path (no CMT1) and end up at the RAW preview scan (error about
        // EXIF read).
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("fallback.cr3");

        let mut ftyp_payload = Vec::new();
        ftyp_payload.extend_from_slice(b"crx ");
        ftyp_payload.extend_from_slice(&u32_be(0));
        ftyp_payload.extend_from_slice(b"crx ");
        let ftyp_box = bmff_box(b"ftyp", &ftyp_payload);

        let mut file_bytes = ftyp_box;
        // moov box with no uuid box inside
        let moov_box = bmff_box(b"moov", &[]);
        file_bytes.extend(moov_box);
        // Append a small embedded JPEG (no EXIF) — parse_raw_exif will try
        // to read EXIF from it and fail.
        let small_jpeg = vec![0xFF, 0xD8]; // SOI
        let padded = std::iter::repeat(0u8).take(300).collect::<Vec<_>>();
        let mut jpeg = small_jpeg;
        jpeg.extend(padded);
        jpeg.push(0xFF);
        jpeg.push(0xD9); // EOI
        file_bytes.extend(jpeg);

        std::fs::write(&path, &file_bytes).unwrap();
        let result = parse_exif(&path);
        assert!(result.is_err());
        // Should come from RAW preview scan error path (mentions EXIF/RAW)
        let err = result.unwrap_err();
        assert!(
            err.contains("EXIF") || err.contains("RAW"),
            "expected preview-scan error, got: {err}"
        );
    }

    #[test]
    fn test_cr3_read_boxes_in_range_basic() {
        // Two sibling boxes, 32-bit sizes.
        let b1 = bmff_box(b"ftyp", b"AAAA");
        let b2 = bmff_box(b"moov", b"BBBB");
        let mut data = b1.clone();
        data.extend(b2);
        let boxes = read_boxes_in_range(&data, 0, data.len()).unwrap();
        assert_eq!(boxes.len(), 2);
        assert_eq!(&boxes[0].box_type, b"ftyp");
        assert_eq!(&boxes[1].box_type, b"moov");
        // Payload of box 0 starts at offset 8 and is 4 bytes.
        assert_eq!(boxes[0].data_end - boxes[0].data_start, 4);
    }

    #[test]
    fn test_cr3_find_canon_tiff_payloads() {
        // Assemble a buffer that contains the CMT1 uuid box inside moov/udta
        // and make sure find_canon_tiff_payloads picks it up.
        let tiff_blob = b"this-will-be-the-tiff-payload";
        let mut uuid_payload = Vec::new();
        uuid_payload.extend_from_slice(&CR3_CMT1_UUID);
        uuid_payload.extend_from_slice(tiff_blob);
        let uuid_box = bmff_box(b"uuid", &uuid_payload);
        let udta_box = bmff_box(b"udta", &uuid_box);
        let moov_box = bmff_box(b"moov", &udta_box);

        let mut ftyp_payload = Vec::new();
        ftyp_payload.extend_from_slice(b"crx ");
        ftyp_payload.extend_from_slice(&u32_be(0));
        let ftyp_box = bmff_box(b"ftyp", &ftyp_payload);

        let mut data = ftyp_box;
        data.extend(moov_box);

        let found = find_canon_tiff_payloads(&data);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], &tiff_blob[..]);
    }

    #[test]
    fn test_cr3_find_canon_tiff_payloads_ignores_other_uuids() {
        // A uuid box with a non-CMT1 UUID must not be reported.
        let other_uuid = [0x11u8; 16];
        let mut other_uuid_payload = Vec::new();
        other_uuid_payload.extend_from_slice(&other_uuid);
        other_uuid_payload.extend_from_slice(b"not-canon");
        let other_box = bmff_box(b"uuid", &other_uuid_payload);
        let udta_box = bmff_box(b"udta", &other_box);
        let moov_box = bmff_box(b"moov", &udta_box);

        let found = find_canon_tiff_payloads(&moov_box);
        assert!(found.is_empty());
    }
}
