use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use exif::{In, Tag};

use super::ExifData;

/// Parse EXIF data from an image file
pub fn parse_exif<P: AsRef<Path>>(path: P) -> Result<ExifData, String> {
    let path = path.as_ref();

    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mut bufreader = BufReader::new(file);

    let exif_reader = exif::Reader::new();
    let exif = exif_reader
        .read_from_container(&mut bufreader)
        .map_err(|e| format!("Failed to read EXIF: {}", e))?;

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
    use tempfile::NamedTempFile;

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
}
