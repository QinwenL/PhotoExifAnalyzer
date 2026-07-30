use std::collections::HashMap;

use super::ExifData;
use super::scanner::ScanResult;

/// Statistics for camera models
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CameraStats {
    /// Camera make and model with count
    pub cameras: Vec<StatItem>,
    /// Total number of images with camera info
    pub total: usize,
}

/// Statistics for lenses
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LensStats {
    /// Lens model with count
    pub lenses: Vec<StatItem>,
    /// Total number of images with lens info
    pub total: usize,
}

/// Statistics for focal lengths
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FocalLengthStats {
    /// Focal length ranges with count
    pub ranges: Vec<FocalRange>,
    /// Total number of images with focal length info
    pub total: usize,
}

/// A stat item with name and count
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatItem {
    /// Item name (e.g., "Canon EOS R5", "RF 24-70mm F2.8L IS USM")
    pub name: String,
    /// Number of occurrences
    pub count: usize,
    /// Percentage of total
    pub percentage: f64,
}

/// Focal length range with count
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FocalRange {
    /// Range label (e.g., "24-35mm", "50mm")
    pub label: String,
    /// Minimum focal length in range
    pub min: f64,
    /// Maximum focal length in range
    pub max: f64,
    /// Number of occurrences
    pub count: usize,
    /// Percentage of total
    pub percentage: f64,
}

/// Filter criteria for statistics
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct FilterCriteria {
    /// Camera models to include
    pub cameras: Option<Vec<String>>,
    /// Lenses to include
    pub lenses: Option<Vec<String>>,
    /// Focal length range (min, max)
    pub focal_length: Option<(f64, f64)>,
    /// Aperture range (min, max)
    pub aperture: Option<(f64, f64)>,
    /// ISO range (min, max)
    pub iso: Option<(u32, u32)>,
    /// Exposure time range (min, max) in seconds
    pub exposure_time: Option<(f64, f64)>,
    /// Date range (start, end) in ISO format (YYYY-MM-DD)
    pub date_range: Option<(String, String)>,
    /// Filter mode: true = AND (all conditions must match), false = OR (any condition)
    pub and_mode: bool,
}

/// Calculate camera statistics from scan results
pub fn calculate_camera_stats(results: &[ScanResult]) -> CameraStats {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for result in results {
        if let Some(ref make) = result.exif.make {
            if let Some(ref model) = result.exif.model {
                let key = format!("{} {}", make, model);
                *counts.entry(key).or_insert(0) += 1;
            } else {
                *counts.entry(make.clone()).or_insert(0) += 1;
            }
        }
    }

    let total: usize = counts.values().sum();
    let mut cameras: Vec<StatItem> = counts
        .into_iter()
        .map(|(name, count)| StatItem {
            name,
            count,
            percentage: if total > 0 {
                (count as f64 / total as f64) * 100.0
            } else {
                0.0
            },
        })
        .collect();

    cameras.sort_by(|a, b| b.count.cmp(&a.count));

    CameraStats { cameras, total }
}

/// Calculate lens statistics from scan results
pub fn calculate_lens_stats(results: &[ScanResult]) -> LensStats {
    let mut counts: HashMap<String, usize> = HashMap::new();

    for result in results {
        if let Some(ref lens) = result.exif.lens_model {
            *counts.entry(lens.clone()).or_insert(0) += 1;
        }
    }

    let total: usize = counts.values().sum();
    let mut lenses: Vec<StatItem> = counts
        .into_iter()
        .map(|(name, count)| StatItem {
            name,
            count,
            percentage: if total > 0 {
                (count as f64 / total as f64) * 100.0
            } else {
                0.0
            },
        })
        .collect();

    lenses.sort_by(|a, b| b.count.cmp(&a.count));

    LensStats { lenses, total }
}

/// Calculate focal length statistics from scan results
pub fn calculate_focal_length_stats(results: &[ScanResult]) -> FocalLengthStats {
    let mut counts: HashMap<String, (f64, f64, usize)> = HashMap::new();

    // Define focal length ranges
    let ranges = vec![
        ("Ultra Wide <14mm", 0.0, 14.0),
        ("Wide 14-24mm", 14.0, 24.0),
        ("Standard 24-50mm", 24.0, 50.0),
        ("Medium Tele 50-100mm", 50.0, 100.0),
        ("Tele 100-200mm", 100.0, 200.0),
        ("Super Tele >200mm", 200.0, 1000.0),
    ];

    for result in results {
        if let Some(focal) = result.exif.focal_length {
            for (label, min, max) in &ranges {
                if focal >= *min && focal < *max {
                    let entry = counts.entry(label.to_string()).or_insert((*min, *max, 0));
                    entry.2 += 1;
                    break;
                }
            }
        }
    }

    let total: usize = counts.values().map(|(_, _, c)| c).sum();
    let mut focal_ranges: Vec<FocalRange> = counts
        .into_iter()
        .map(|(label, (min, max, count))| FocalRange {
            label,
            min,
            max,
            count,
            percentage: if total > 0 {
                (count as f64 / total as f64) * 100.0
            } else {
                0.0
            },
        })
        .collect();

    focal_ranges.sort_by(|a, b| a.min.partial_cmp(&b.min).unwrap_or(std::cmp::Ordering::Equal));

    FocalLengthStats {
        ranges: focal_ranges,
        total,
    }
}

/// Filter scan results based on criteria
pub fn filter_results(results: &[ScanResult], criteria: &FilterCriteria) -> Vec<ScanResult> {
    results
        .iter()
        .filter(|r| matches_criteria(&r.exif, criteria))
        .cloned()
        .collect()
}

/// Check if an EXIF record matches the filter criteria
fn matches_criteria(exif: &ExifData, criteria: &FilterCriteria) -> bool {
    let mut checks = Vec::new();

    // Camera filter
    if let Some(ref cameras) = criteria.cameras {
        let combined = match (&exif.make, &exif.model) {
            (Some(ma), Some(mo)) => Some(format!("{} {}", ma, mo)),
            _ => None,
        };
        let matches = combined.as_ref().map(|c| cameras.contains(c)).unwrap_or(false)
            || exif.make.as_ref().map(|m| cameras.contains(m)).unwrap_or(false)
            || exif.model.as_ref().map(|m| cameras.contains(m)).unwrap_or(false);
        checks.push(matches);
    }

    // Lens filter
    if let Some(ref lenses) = criteria.lenses {
        let matches = exif.lens_model.as_ref().map(|l| lenses.contains(l)).unwrap_or(false);
        checks.push(matches);
    }

    // Focal length filter
    if let Some((min, max)) = criteria.focal_length {
        let matches = exif.focal_length.map(|f| f >= min && f <= max).unwrap_or(false);
        checks.push(matches);
    }

    // Aperture filter
    if let Some((min, max)) = criteria.aperture {
        let matches = exif.aperture.map(|a| a >= min && a <= max).unwrap_or(false);
        checks.push(matches);
    }

    // ISO filter
    if let Some((min, max)) = criteria.iso {
        let matches = exif.iso.map(|i| i >= min && i <= max).unwrap_or(false);
        checks.push(matches);
    }

    // Exposure time filter
    if let Some((min, max)) = criteria.exposure_time {
        let matches = exif.exposure_time.map(|e| e >= min && e <= max).unwrap_or(false);
        checks.push(matches);
    }

    // Date range filter
    if let Some((ref start, ref end)) = criteria.date_range {
        let matches = exif.datetime_original.as_ref().map(|dt| {
            let date_part = dt.split('T').next().unwrap_or(dt);
            date_part >= start.as_str() && date_part <= end.as_str()
        }).unwrap_or(false);
        checks.push(matches);
    }

    if checks.is_empty() {
        return true;
    }

    if criteria.and_mode {
        checks.iter().all(|&c| c)
    } else {
        checks.iter().any(|&c| c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_result(make: &str, model: &str, lens: &str, focal: f64) -> ScanResult {
        ScanResult {
            path: PathBuf::from("/test/photo.jpg"),
            exif: ExifData {
                make: Some(make.to_string()),
                model: Some(model.to_string()),
                lens_model: Some(lens.to_string()),
                focal_length: Some(focal),
                ..Default::default()
            },
            file_size: 1000,
            error: None,
        }
    }

    #[test]
    fn test_camera_stats() {
        let results = vec![
            create_test_result("Canon", "EOS R5", "RF 50mm", 50.0),
            create_test_result("Canon", "EOS R5", "RF 50mm", 50.0),
            create_test_result("Nikon", "Z6", "NIKKOR Z 50mm", 50.0),
        ];

        let stats = calculate_camera_stats(&results);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.cameras.len(), 2);
        assert_eq!(stats.cameras[0].name, "Canon EOS R5");
        assert_eq!(stats.cameras[0].count, 2);
    }

    #[test]
    fn test_lens_stats() {
        let results = vec![
            create_test_result("Canon", "R5", "RF 50mm F1.2", 50.0),
            create_test_result("Canon", "R5", "RF 50mm F1.2", 50.0),
            create_test_result("Canon", "R5", "RF 24-70mm F2.8", 24.0),
        ];

        let stats = calculate_lens_stats(&results);
        assert_eq!(stats.total, 3);
        assert_eq!(stats.lenses.len(), 2);
        assert_eq!(stats.lenses[0].name, "RF 50mm F1.2");
        assert_eq!(stats.lenses[0].count, 2);
    }

    #[test]
    fn test_focal_length_stats() {
        let results = vec![
            create_test_result("Canon", "R5", "RF 50mm", 50.0),
            create_test_result("Canon", "R5", "RF 50mm", 50.0),
            create_test_result("Canon", "R5", "RF 24-70mm", 24.0),
            create_test_result("Canon", "R5", "RF 100mm", 100.0),
        ];

        let stats = calculate_focal_length_stats(&results);
        assert_eq!(stats.total, 4);
        assert!(!stats.ranges.is_empty());
    }

    #[test]
    fn test_filter_criteria_and_mode() {
        let results = vec![
            create_test_result("Canon", "EOS R5", "RF 50mm", 50.0),
            create_test_result("Canon", "EOS R5", "RF 24-70mm", 24.0),
            create_test_result("Nikon", "Z6", "NIKKOR Z 50mm", 50.0),
        ];

        let criteria = FilterCriteria {
            cameras: Some(vec!["Canon EOS R5".to_string()]),
            focal_length: Some((40.0, 60.0)),
            and_mode: true,
            ..Default::default()
        };

        let filtered = filter_results(&results, &criteria);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].exif.model.as_deref(), Some("EOS R5"));
    }

    #[test]
    fn test_filter_criteria_or_mode() {
        let results = vec![
            create_test_result("Canon", "EOS R5", "RF 50mm", 50.0),
            create_test_result("Canon", "EOS R5", "RF 24-70mm", 24.0),
            create_test_result("Nikon", "Z6", "NIKKOR Z 50mm", 50.0),
        ];

        let criteria = FilterCriteria {
            cameras: Some(vec!["Nikon Z6".to_string()]),
            focal_length: Some((40.0, 60.0)),
            and_mode: false,
            ..Default::default()
        };

        let filtered = filter_results(&results, &criteria);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_date_range() {
        let mut r1 = create_test_result("Canon", "EOS R5", "RF 50mm", 50.0);
        r1.exif.datetime_original = Some("2024-01-15T10:30:00".to_string());

        let mut r2 = create_test_result("Canon", "EOS R5", "RF 50mm", 50.0);
        r2.exif.datetime_original = Some("2024-02-20T14:00:00".to_string());

        let mut r3 = create_test_result("Canon", "EOS R5", "RF 50mm", 50.0);
        r3.exif.datetime_original = Some("2024-03-10T09:15:00".to_string());

        let results = vec![r1, r2, r3];

        let criteria = FilterCriteria {
            date_range: Some(("2024-02-01".to_string(), "2024-02-28".to_string())),
            and_mode: true,
            ..Default::default()
        };

        let filtered = filter_results(&results, &criteria);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].exif.datetime_original.as_deref(), Some("2024-02-20T14:00:00"));
    }

    #[test]
    fn test_filter_iso_aperture_or_mode() {
        // Tests ISO and aperture filters in OR mode (Oracle recommended gap test)
        let mut r1 = create_test_result("Canon", "R5", "RF 50mm", 50.0);
        r1.exif.iso = Some(100);
        r1.exif.aperture = Some(1.2);

        let mut r2 = create_test_result("Canon", "R5", "RF 50mm", 50.0);
        r2.exif.iso = Some(6400);
        r2.exif.aperture = Some(8.0);

        let mut r3 = create_test_result("Nikon", "Z6", "NIKKOR 50mm", 50.0);
        r3.exif.iso = Some(3200);
        r3.exif.aperture = Some(4.0);

        let results = vec![r1, r2, r3];

        // OR mode: ISO in [100,200] OR aperture in [1.0,2.0]
        let criteria = FilterCriteria {
            iso: Some((100, 200)),
            aperture: Some((1.0, 2.0)),
            and_mode: false,
            ..Default::default()
        };

        let filtered = filter_results(&results, &criteria);
        // r1 matches both, r2 matches neither, r3 matches neither
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].exif.iso, Some(100));

        // AND mode: must match BOTH
        let criteria_and = FilterCriteria {
            iso: Some((100, 200)),
            aperture: Some((1.0, 2.0)),
            and_mode: true,
            ..Default::default()
        };

        let filtered_and = filter_results(&results, &criteria_and);
        // Only r1 matches both conditions
        assert_eq!(filtered_and.len(), 1);
    }

    #[test]
    fn test_camera_stats_make_only() {
        // Oracle recommended: test make-only path (model == None)
        let results = vec![
            ScanResult {
                path: PathBuf::from("/test/a.jpg"),
                exif: ExifData {
                    make: Some("Canon".to_string()),
                    model: None,
                    ..Default::default()
                },
                file_size: 1000,
                error: None,
            },
            ScanResult {
                path: PathBuf::from("/test/b.jpg"),
                exif: ExifData {
                    make: Some("Canon".to_string()),
                    model: Some("EOS R5".to_string()),
                    ..Default::default()
                },
                file_size: 1000,
                error: None,
            },
        ];

        let stats = calculate_camera_stats(&results);
        assert_eq!(stats.total, 2);
        // "Canon" (make-only) and "Canon EOS R5" should be separate entries
        assert_eq!(stats.cameras.len(), 2);
    }

    #[test]
    fn test_filter_empty_results() {
        let results: Vec<ScanResult> = vec![];
        let criteria = FilterCriteria {
            cameras: Some(vec!["Canon".to_string()]),
            and_mode: false,
            ..Default::default()
        };
        let filtered = filter_results(&results, &criteria);
        assert!(filtered.is_empty());
    }
}
