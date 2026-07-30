# src-tauri/

Rust 后端，通过 Tauri 命令暴露给前端。

## STRUCTURE

```
src-tauri/
├── src/
│   ├── lib.rs          # 13 个 Tauri 命令 + 全局状态
│   ├── main.rs         # 入口（Windows 子系统隐藏）
│   └── exif/           # 核心业务逻辑（见 exif/AGENTS.md）
├── tests/
│   └── integration_test.rs  # 9 个集成测试
├── Cargo.toml          # 依赖：kamadak-exif, rayon, rusqlite, trash
└── tauri.conf.json     # Tauri 配置：allowlist, 窗口, 打包
```

## WHERE TO LOOK

| 任务 | 位置 | 备注 |
|------|------|------|
| 添加 Tauri 命令 | `src/lib.rs` | `#[tauri::command]` + `generate_handler!` |
| 修改全局状态 | `src/lib.rs:16-20` | `lazy_static!` |
| 修改窗口配置 | `tauri.conf.json` | allowlist, CSP, 窗口尺寸 |
| 添加 Rust 依赖 | `Cargo.toml` | edition 2021 |

## COMMANDS (lib.rs)

| 命令 | 用途 | 前端调用 |
|------|------|----------|
| `scan_images_with_progress` | 扫描+进度 | `store.ts` ✅ |
| `cancel_scan` | 取消扫描 | `store.ts` ✅ |
| `get_camera_stats` | 相机统计 | `store.ts` ✅ |
| `get_lens_stats` | 镜头统计 | `store.ts` ✅ |
| `get_focal_length_stats` | 焦段统计 | `store.ts` ✅ |
| `filter_images` | 过滤结果 | `store.ts` ✅ |
| `delete_image` | 单文件删除 | `ImageDetail.tsx` ✅ |
| `delete_images_with_progress` | 批量删除+进度 | `store.ts` ✅ |
| `get_image_data` | Base64 缩略图 | `Thumbnail.tsx` ✅ |
| `scan_images` | 基础扫描 | ⚠️ 未使用 |
| `delete_images` | 批量删除 | ⚠️ 未使用 |
| `get_thumbnail` | 缩略图路径 | ⚠️ 未使用 |
| `export_statistics` | JSON 导出 | ⚠️ 未使用 |

## CONVENTIONS

- **进度事件**：`window.emit("event_name", f64)`（scan_progress / delete_progress）
- **缓存清理**：`cleanup_caches_async()` 在后台线程执行，删除后自动清理缓存

## ANTI-PATTERNS

- **3 个未使用命令**：`delete_images`、`get_thumbnail`、`export_statistics`（`scan_images` 在 store 有 action 但无组件调用）
- **Bundle icon 为空**：`"icon": []`（生产打包需配置）
