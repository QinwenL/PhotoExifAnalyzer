pub mod cache;
#[cfg(target_os = "windows")]
pub mod heic;
pub mod file_ops;
pub mod parser;
pub mod scanner;
pub mod stats;
pub mod thumbnail;

use serde::{Deserialize, Serialize};

/// EXIF data extracted from an image
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExifData {
    /// Camera manufacturer (e.g., "Canon", "Nikon")
    pub make: Option<String>,
    /// Camera model (e.g., "Canon EOS R5", "Nikon Z6")
    pub model: Option<String>,
    /// Lens model (e.g., "RF 24-70mm F2.8L IS USM")
    pub lens_model: Option<String>,
    /// Focal length in mm (e.g., 50.0)
    pub focal_length: Option<f64>,
    /// Aperture value (e.g., 2.8)
    pub aperture: Option<f64>,
    /// ISO speed (e.g., 100)
    pub iso: Option<u32>,
    /// Exposure time in seconds (e.g., 0.001 for 1/1000s)
    pub exposure_time: Option<f64>,
    /// Exposure program (e.g., "Aperture Priority")
    pub exposure_program: Option<String>,
    /// Metering mode (e.g., "Evaluative")
    pub metering_mode: Option<String>,
    /// Flash fired (e.g., true/false)
    pub flash: Option<bool>,
    /// White balance (e.g., "Auto", "Daylight")
    pub white_balance: Option<String>,
    /// Image width in pixels
    pub image_width: Option<u32>,
    /// Image height in pixels
    pub image_height: Option<u32>,
    /// Date/time original (e.g., "2024-01-15 14:30:00")
    pub datetime_original: Option<String>,
    /// GPS latitude in decimal degrees
    pub gps_latitude: Option<f64>,
    /// GPS longitude in decimal degrees
    pub gps_longitude: Option<f64>,
}

impl ExifData {
    /// Create a new empty ExifData
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the EXIF data is empty (all fields are None)
    pub fn is_empty(&self) -> bool {
        self.make.is_none()
            && self.model.is_none()
            && self.lens_model.is_none()
            && self.focal_length.is_none()
            && self.aperture.is_none()
            && self.iso.is_none()
            && self.exposure_time.is_none()
    }

    /// Format the camera display name from make and model.
    ///
    /// - Both present → `"Canon EOS R5"`
    /// - Only make    → `"Canon"`
    /// - Only model   → `"iPhone 15 Pro"`
    /// - Neither      → `None`
    ///
    /// This is the single source of truth for the `make + model` formatting
    /// that was previously duplicated in `stats::calculate_camera_stats`,
    /// `stats::matches_criteria`, `lib::export_statistics`, and the frontend.
    pub fn camera_name(&self) -> Option<String> {
        match (&self.make, &self.model) {
            (Some(make), Some(model)) => Some(format!("{} {}", make, model)),
            (Some(make), None) => Some(make.clone()),
            (None, Some(model)) => Some(model.clone()),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exif_data_creation() {
        let exif = ExifData::new();
        assert!(exif.is_empty());
        assert!(exif.make.is_none());
        assert!(exif.model.is_none());
        assert!(exif.focal_length.is_none());
    }

    #[test]
    fn test_exif_data_with_values() {
        let exif = ExifData {
            make: Some("Canon".to_string()),
            model: Some("Canon EOS R5".to_string()),
            focal_length: Some(50.0),
            aperture: Some(2.8),
            iso: Some(100),
            ..Default::default()
        };

        assert!(!exif.is_empty());
        assert_eq!(exif.make.as_deref(), Some("Canon"));
        assert_eq!(exif.model.as_deref(), Some("Canon EOS R5"));
        assert_eq!(exif.focal_length, Some(50.0));
        assert_eq!(exif.aperture, Some(2.8));
        assert_eq!(exif.iso, Some(100));
    }

    #[test]
    fn test_exif_data_is_empty_partial() {
        let exif = ExifData {
            make: Some("Canon".to_string()),
            ..Default::default()
        };

        assert!(!exif.is_empty());
    }

    #[test]
    fn test_camera_name_both_make_and_model() {
        let exif = ExifData {
            make: Some("Canon".to_string()),
            model: Some("EOS R5".to_string()),
            ..Default::default()
        };
        assert_eq!(exif.camera_name().as_deref(), Some("Canon EOS R5"));
    }

    #[test]
    fn test_camera_name_make_only() {
        let exif = ExifData {
            make: Some("Sigma".to_string()),
            ..Default::default()
        };
        assert_eq!(exif.camera_name().as_deref(), Some("Sigma"));
    }

    #[test]
    fn test_camera_name_model_only() {
        let exif = ExifData {
            model: Some("iPhone 15 Pro".to_string()),
            ..Default::default()
        };
        assert_eq!(exif.camera_name().as_deref(), Some("iPhone 15 Pro"));
    }

    #[test]
    fn test_camera_name_neither() {
        let exif = ExifData::new();
        assert!(exif.camera_name().is_none());
    }
}
