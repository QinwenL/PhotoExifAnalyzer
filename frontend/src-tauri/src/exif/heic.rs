//! HEIC/HEIF/HIF image decoding via Windows Imaging Component (WIC).
//!
//! WIC is built into Windows and decodes HEIC/HEIF natively when the codec
//! is installed (included by default on Windows 10 1809+ and Windows 11,
//! or available from the Microsoft Store as "HEIF Image Extensions").

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use image::DynamicImage;
use windows::core::PCWSTR;
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat24bppRGB, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};

/// Decode a HEIC/HEIF/HIF image file using WIC.
///
/// Returns an `Rgb8` `DynamicImage` on success. All errors are prefixed with
/// "HEIC" so callers can identify the format that failed.
pub fn decode(path: &Path) -> Result<DynamicImage, String> {
    let co_init_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).is_ok() };

    let result = decode_inner(path);

    if co_init_ok {
        unsafe { CoUninitialize() };
    }

    result.map_err(|e| format!("HEIC decode failed: {}", e))
}

fn decode_inner(path: &Path) -> Result<DynamicImage, String> {
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("WIC: failed to create imaging factory: {}", e))?;

        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let decoder = factory
            .CreateDecoderFromFilename(
                PCWSTR::from_raw(wide.as_ptr()),
                None,
                GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .map_err(|e| {
                format!(
                    "WIC: cannot open HEIC file (codec may be missing; install \
                     \"HEIF Image Extensions\" from Microsoft Store): {}",
                    e
                )
            })?;

        let frame = decoder
            .GetFrame(0)
            .map_err(|e| format!("WIC: failed to get HEIC frame: {}", e))?;

        let converter = factory
            .CreateFormatConverter()
            .map_err(|e| format!("WIC: failed to create format converter: {}", e))?;

        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat24bppRGB,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|e| format!("WIC: failed to initialize format converter: {}", e))?;

        let mut width = 0u32;
        let mut height = 0u32;
        converter
            .GetSize(&mut width, &mut height)
            .map_err(|e| format!("WIC: failed to get image size: {}", e))?;

        if width == 0 || height == 0 {
            return Err(format!(
                "WIC: decoded HEIC has zero dimensions ({}x{})",
                width, height
            ));
        }

        let stride = width as usize * 3;
        let buf_size = stride
            .checked_mul(height as usize)
            .ok_or_else(|| "WIC: pixel buffer size overflow".to_string())?;
        let mut buffer = vec![0u8; buf_size];

        converter
            .CopyPixels(std::ptr::null(), stride as u32, &mut buffer)
            .map_err(|e| format!("WIC: failed to copy pixels: {}", e))?;

        let img = image::RgbImage::from_raw(width, height, buffer)
            .ok_or_else(|| "WIC: failed to build RgbImage from pixel buffer".to_string())?;

        Ok(DynamicImage::ImageRgb8(img))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_decode_invalid_heic_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let fake = temp_dir.path().join("fake.heic");
        std::fs::write(&fake, b"this is not a valid HEIC file").unwrap();

        let result = decode(&fake);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HEIC"));
    }

    #[test]
    fn test_decode_invalid_hif_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let fake = temp_dir.path().join("photo.hif");
        std::fs::write(&fake, b"not real hif data").unwrap();

        let result = decode(&fake);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HEIC"));
    }

    #[test]
    #[ignore = "requires a real HEIC/HIF file; set HEIC_TEST_FILE env var"]
    fn test_decode_real_heic_file() {
        let path = std::env::var("HEIC_TEST_FILE")
            .expect("Set HEIC_TEST_FILE to a real .heic/.hif file path");
        let img = decode(std::path::Path::new(&path)).unwrap();
        assert!(img.width() > 0);
        assert!(img.height() > 0);
    }
}