//! RAW 文件 JPEG preview 定位（零拷贝 + SIMD 加速）。
//!
//! 统一 `parser.rs`（EXIF 提取）和 `thumbnail.rs`（缩略图生成）共用的
//! 嵌入式 JPEG preview 扫描逻辑，用 `memmap2` 零拷贝映射 + `memchr`
//! SIMD 加速替代 `std::fs::read` + 逐字节 `windows(2)` 扫描。

use std::path::Path;

/// 嵌入式 JPEG preview 的字节位置信息。
///
/// `offset` 是相对文件起始的字节偏移，`length` 是 JPEG 数据长度
/// （含 SOI 和 EOI 标记）。调用方可以用 `seek + read_exact` 只读
/// 这一段字节，无需把整个 RAW 文件读入内存。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JpegPreview {
    pub offset: u64,
    pub length: usize,
}

/// 按格式确定的前向扫描上限（字节）。
///
/// 大多数 TIFF-based RAW（NEF/ARW/DNG/CR2）的嵌入式 preview 都在
/// 文件前 8MB 内；CR3 的 CMT1 EXIF 在 `moov` box 内，也在前 8MB。
const HEAD_SCAN_LIMIT: u64 = 8 * 1024 * 1024;

/// 按格式确定的后向扫描上限（字节）。
///
/// CR2 的全分辨率 preview 存在文件末尾的 IFD 链中，通常距 EOF
/// 几 MB 内。8MB 覆盖了所有 Canon 机型的实测范围。
const TAIL_SCAN_LIMIT: u64 = 8 * 1024 * 1024;

/// 在 RAW 文件中查找最大的嵌入式 JPEG preview。
///
/// 流程：
/// 1. 用 `memmap2` 只读映射文件（不读入堆）。
/// 2. 按扩展名确定优先扫描区间（前 8MB 或后 8MB）。
/// 3. 在优先区间内用 `memchr`（SIMD）扫描所有 JPEG SOI 标记。
/// 4. 对每个 SOI，用 `memchr` 找对应的 EOI，记录最大的完整 JPEG。
/// 5. 优先区间未命中则回退全文件扫描（保证不漏）。
///
/// 返回 `Ok(Some(preview))` 找到时；`Ok(None)` 没找到时；
/// `Err` 打开/映射文件失败时。
pub fn find_largest_jpeg_preview(path: &Path) -> Result<Option<JpegPreview>, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open RAW file: {}", e))?;
    let file_len = file
        .metadata()
        .map_err(|e| format!("Failed to get file metadata: {}", e))?
        .len();

    if file_len == 0 {
        return Ok(None);
    }

    let mmap = unsafe { memmap2::Mmap::map(&file) }
        .map_err(|e| format!("Failed to mmap file: {}", e))?;
    let data: &[u8] = &mmap[..];

    // 优先区间扫描
    let (start, end) = raw_preview_scan_range(path, file_len);
    if let Some(preview) = scan_range_for_largest_jpeg(data, start as usize, end as usize) {
        return Ok(Some(preview));
    }

    // 回退全文件扫描（保证不漏 preview）
    Ok(scan_range_for_largest_jpeg(data, 0, data.len()))
}

/// 在 `data[start..end]` 内查找最大的完整 JPEG（SOI..EOI）。
///
/// 用 `memchr` 定位 `0xFF` 字节，再检查紧跟的字节是否为 `0xD8`（SOI）
/// 或 `0xD9`（EOI）。SIMD 加速，一次扫描 16-32 字节。
fn scan_range_for_largest_jpeg(data: &[u8], start: usize, end: usize) -> Option<JpegPreview> {
    let end = end.min(data.len());
    if start >= end {
        return None;
    }

    let slice = &data[start..end];
    let mut best: Option<JpegPreview> = None;
    let mut best_len: usize = 0;

    let mut pos = 0;
    while pos < slice.len().saturating_sub(1) {
        // 用 memchr（SIMD）找下一个 0xFF
        let rel = memchr::memchr(0xFF, &slice[pos..])?;
        let soi_rel = pos + rel;

        // 检查是否为 SOI (FF D8)
        if soi_rel + 1 >= slice.len() {
            break;
        }
        if slice[soi_rel + 1] != 0xD8 {
            pos = soi_rel + 1;
            continue;
        }

        // 找到 SOI，从 SOI+2 开始找 EOI (FF D9)
        let eoi_rel = find_jpeg_eoi_memchr(slice, soi_rel + 2);
        if let Some(eoi_pos) = eoi_rel {
            let jpeg_len = eoi_pos + 1 - soi_rel; // 含 SOI 和 EOI
            if jpeg_len > best_len {
                best_len = jpeg_len;
                best = Some(JpegPreview {
                    offset: (start + soi_rel) as u64,
                    length: jpeg_len,
                });
            }
            pos = eoi_pos + 1;
        } else {
            // SOI 无对应 EOI，跳过
            pos = soi_rel + 2;
        }
    }

    best
}

/// 在 `data[from..]` 中查找 JPEG EOI 标记 `FF D9`。
///
/// 返回 `D9` 字节的绝对索引（EOI 的最后一个字节），或 `None`。
/// 用 `memchr` 加速 `0xFF` 搜索，再检查下一个字节是否为 `0xD9`。
fn find_jpeg_eoi_memchr(data: &[u8], from: usize) -> Option<usize> {
    let mut pos = from;
    while pos < data.len().saturating_sub(1) {
        let rel = memchr::memchr(0xFF, &data[pos..])?;
        let ff_pos = pos + rel;
        if ff_pos + 1 >= data.len() {
            return None;
        }
        if data[ff_pos + 1] == 0xD9 {
            return Some(ff_pos + 1);
        }
        pos = ff_pos + 1;
    }
    None
}

/// 按文件扩展名返回优先扫描区间 `(start, end)`（字节偏移，左闭右开）。
///
/// 返回的区间已按 `file_len` 裁剪，保证 `start <= end <= file_len`。
fn raw_preview_scan_range(path: &Path, file_len: u64) -> (u64, u64) {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    // CR2 的全分辨率 preview 在文件末尾的 IFD 链中
    if ext == "cr2" {
        let start = file_len.saturating_sub(TAIL_SCAN_LIMIT);
        return (start, file_len);
    }

    // CR3/NEF/ARW/DNG 等的 preview 在文件前部
    let head_formats = [
        "cr3", "nef", "nrw", "arw", "srf", "sr2", "orf", "raf", "rw2", "pef", "dng", "raw",
        "rwl", "3fr", "kdc", "dcr", "mrw", "srw", "x3f", "bay",
    ];
    if head_formats.contains(&ext.as_str()) {
        let end = HEAD_SCAN_LIMIT.min(file_len);
        return (0, end);
    }

    // 未知格式全扫
    (0, file_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// 构造测试用 RAW 文件：前导 padding + 一段 JPEG 数据。
    fn create_raw_with_jpeg(dir: &Path, name: &str, padding: usize, jpeg: &[u8]) -> PathBuf {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&vec![0u8; padding]).unwrap();
        file.write_all(jpeg).unwrap();
        path
    }

    /// 构造一个最小但结构合法的 JPEG（SOI + 少量数据 + EOI）。
    fn minimal_jpeg(body_len: usize) -> Vec<u8> {
        let mut jpeg = vec![0xFF, 0xD8]; // SOI
        jpeg.extend(std::iter::repeat_n(0xAAu8, body_len));
        jpeg.push(0xFF);
        jpeg.push(0xD9); // EOI
        jpeg
    }

    // ---- find_largest_jpeg_preview 行为测试 ----

    #[test]
    fn test_find_preview_returns_none_when_no_jpeg() {
        let temp_dir = TempDir::new().unwrap();
        let path = create_raw_with_jpeg(temp_dir.path(), "photo.cr2", 1024, &[]);
        // 无 JPEG 数据时返回 None
        let result = find_largest_jpeg_preview(&path).unwrap();
        assert!(result.is_none(), "expected None when no JPEG in file");
    }

    #[test]
    fn test_find_preview_finds_single_jpeg_at_offset() {
        let temp_dir = TempDir::new().unwrap();
        let jpeg = minimal_jpeg(300);
        let padding = 512;
        let path = create_raw_with_jpeg(temp_dir.path(), "photo.cr2", padding, &jpeg);

        let result = find_largest_jpeg_preview(&path).unwrap();
        assert!(result.is_some(), "expected a preview to be found");
        let preview = result.unwrap();
        assert_eq!(preview.offset, padding as u64);
        assert_eq!(preview.length, jpeg.len());
    }

    #[test]
    fn test_find_preview_picks_largest_when_multiple_jpegs() {
        let temp_dir = TempDir::new().unwrap();
        let small = minimal_jpeg(100);
        let large = minimal_jpeg(500);
        let padding = 256;

        let path = temp_dir.path().join("photo.nef");
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&vec![0u8; padding]).unwrap();
        file.write_all(&small).unwrap();
        file.write_all(&large).unwrap();

        let result = find_largest_jpeg_preview(&path).unwrap();
        assert!(result.is_some());
        let preview = result.unwrap();
        assert_eq!(preview.length, large.len(), "must pick the larger JPEG");
        assert_eq!(
            preview.offset,
            (padding + small.len()) as u64,
            "offset must point to the larger JPEG"
        );
    }

    #[test]
    fn test_find_preview_returns_none_for_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("empty.cr2");
        std::fs::File::create(&path).unwrap();
        let result = find_largest_jpeg_preview(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_find_preview_errors_on_nonexistent_file() {
        let result = find_largest_jpeg_preview(Path::new("/nonexistent/photo.cr2"));
        assert!(result.is_err(), "opening a nonexistent file must error");
    }

    #[test]
    fn test_find_preview_finds_jpeg_at_tail_for_cr2() {
        // CR2 preview 通常在文件末尾。构造一个头部无 JPEG、
        // 尾部有 JPEG 的文件，验证尾部扫描能命中。
        let temp_dir = TempDir::new().unwrap();
        let jpeg = minimal_jpeg(300);
        // 头部 padding 远超 HEAD_SCAN_LIMIT（8MB），保证前向扫描不会命中
        let padding = (HEAD_SCAN_LIMIT + 1024) as usize;
        let path = create_raw_with_jpeg(temp_dir.path(), "photo.cr2", padding, &jpeg);

        let result = find_largest_jpeg_preview(&path).unwrap();
        assert!(result.is_some(), "CR2 tail preview must be found via fallback");
        let preview = result.unwrap();
        assert_eq!(preview.offset, padding as u64);
        assert_eq!(preview.length, jpeg.len());
    }

    #[test]
    fn test_find_preview_finds_jpeg_in_head_for_cr3() {
        // CR3 preview 在 moov box 内，文件前部。构造前部有 JPEG 的文件。
        let temp_dir = TempDir::new().unwrap();
        let jpeg = minimal_jpeg(300);
        let path = create_raw_with_jpeg(temp_dir.path(), "photo.cr3", 1024, &jpeg);

        let result = find_largest_jpeg_preview(&path).unwrap();
        assert!(result.is_some());
        let preview = result.unwrap();
        assert_eq!(preview.offset, 1024);
        assert_eq!(preview.length, jpeg.len());
    }

    // ---- raw_preview_scan_range 单元测试 ----

    #[test]
    fn test_scan_range_cr2_returns_tail() {
        let file_len: u64 = 80 * 1024 * 1024; // 80MB
        let (start, end) = raw_preview_scan_range(Path::new("photo.cr2"), file_len);
        assert_eq!(end, file_len, "CR2 scan range must end at EOF");
        assert_eq!(
            file_len - start,
            TAIL_SCAN_LIMIT,
            "CR2 scan range must be the tail TAIL_SCAN_LIMIT bytes"
        );
    }

    #[test]
    fn test_scan_range_cr3_returns_head() {
        let file_len: u64 = 80 * 1024 * 1024;
        let (start, end) = raw_preview_scan_range(Path::new("photo.cr3"), file_len);
        assert_eq!(start, 0, "CR3 scan range must start at 0");
        assert_eq!(end, HEAD_SCAN_LIMIT, "CR3 scan range must be the head HEAD_SCAN_LIMIT bytes");
    }

    #[test]
    fn test_scan_range_nef_returns_head() {
        let file_len: u64 = 50 * 1024 * 1024;
        let (start, end) = raw_preview_scan_range(Path::new("photo.nef"), file_len);
        assert_eq!(start, 0);
        assert_eq!(end, HEAD_SCAN_LIMIT);
    }

    #[test]
    fn test_scan_range_unknown_extension_returns_full_file() {
        let file_len: u64 = 10 * 1024 * 1024;
        let (start, end) = raw_preview_scan_range(Path::new("photo.xyz"), file_len);
        assert_eq!(start, 0);
        assert_eq!(end, file_len);
    }

    #[test]
    fn test_scan_range_clamps_to_file_len_for_small_files() {
        // 文件比 HEAD_SCAN_LIMIT 小时，区间不能超出文件长度
        let file_len: u64 = 1024 * 1024; // 1MB
        let (start, end) = raw_preview_scan_range(Path::new("photo.cr3"), file_len);
        assert_eq!(start, 0);
        assert_eq!(end, file_len, "scan range must not exceed file length");
    }

    #[test]
    fn test_scan_range_cr2_clamps_for_small_files() {
        let file_len: u64 = 1024 * 1024; // 1MB — 比 TAIL_SCAN_LIMIT 小
        let (start, end) = raw_preview_scan_range(Path::new("photo.cr2"), file_len);
        assert_eq!(start, 0, "start must clamp to 0 when file < TAIL_SCAN_LIMIT");
        assert_eq!(end, file_len);
    }
}
