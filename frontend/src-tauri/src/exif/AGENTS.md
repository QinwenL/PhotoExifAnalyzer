# exif/

核心业务逻辑模块，处理 EXIF 解析、扫描、缓存、统计、文件操作、缩略图。

## STRUCTURE

```
exif/
├── mod.rs        # ExifData 结构体定义（16 个可选字段）
├── parser.rs     # EXIF 提取（kamadak-exif）
├── scanner.rs    # 目录扫描 + rayon 并行 + 缓存集成
├── cache.rs      # SQLite 缓存（rusqlite bundled）
├── stats.rs      # 统计计算 + 多维过滤（AND/OR）
├── file_ops.rs   # 文件删除（trash crate）+ 批量操作
└── thumbnail.rs  # 缩略图生成 + base64 编码
```

## WHERE TO LOOK

| 任务 | 位置 | 备注 |
|------|------|------|
| 修改 EXIF 字段 | `mod.rs:11-45` | `ExifData` 结构体 |
| 修改解析逻辑 | `parser.rs` | `parse_exif()` 函数 |
| 修改扫描/并行 | `scanner.rs` | `scan_directory_with_cache()` |
| 修改缓存 | `cache.rs` | `ExifCache` 类型 |
| 修改统计 | `stats.rs` | `calculate_*_stats()` |
| 修改过滤 | `stats.rs:60-78` | `FilterCriteria` 结构体 |
| 修改删除 | `file_ops.rs` | `delete_file()` / `delete_files()` |
| 修改缩略图 | `thumbnail.rs` | `get_image_base64()` |

## CONVENTIONS

- **错误类型**：`Result<T, String>`，用 `format!("Failed to {}: {}", action, e)`
- **缓存键**：文件路径的 `to_string_lossy()` 字符串
- **缓存验证**：文件修改时间 + 缓存版本号
- **并行**：rayon `par_iter()`，4 线程限制
- **进度回调**：`impl Fn(f64) + Send + Sync + 'static`
- **取消检查**：`impl Fn() -> bool + Send + Sync + 'static`

## ANTI-PATTERNS

- **相机名格式化重复**：`make + model` 在 stats.rs 和 lib.rs 中重复
- **缓存失效由上层处理**：exif 模块自身不处理缓存失效，由 lib.rs 的 `cleanup_caches_async()` 负责
