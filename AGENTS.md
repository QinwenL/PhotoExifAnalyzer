# PhotoExifAnalyzer

**Generated:** 2026-07-28
**Stack:** Tauri v1 + React 19 + Rust (edition 2021)

## OVERVIEW

桌面端照片 EXIF 元数据分析器。扫描目录、提取 EXIF、统计相机/镜头/焦段分布，支持过滤、删除、导出。

## STRUCTURE

```
PhotoExifAnalyzer/
├── frontend/
│   ├── src-tauri/     # Rust 后端（Tauri 命令层）
│   │   ├── src/
│   │   │   ├── lib.rs        # 13 个 Tauri 命令 + 全局状态
│   │   │   ├── main.rs       # 入口，Windows 子系统隐藏
│   │   │   └── exif/         # 核心模块（见 exif/AGENTS.md）
│   │   └── tests/
│   │       └── integration_test.rs
│   ├── src/           # React 前端（见 src/AGENTS.md）
│   ├── package.json
│   └── vite.config.ts
└── openspec/          # 规格文档（非代码）
```

## WHERE TO LOOK

| 任务 | 位置 | 备注 |
|------|------|------|
| 添加 Tauri 命令 | `frontend/src-tauri/src/lib.rs` | `#[tauri::command]` + `generate_handler!` |
| 修改 EXIF 解析 | `frontend/src-tauri/src/exif/parser.rs` | 使用 `kamadak-exif` |
| 修改扫描逻辑 | `frontend/src-tauri/src/exif/scanner.rs` | rayon 并行，4 线程 |
| 修改缓存 | `frontend/src-tauri/src/exif/cache.rs` | SQLite，文件哈希键 |
| 修改前端状态 | `frontend/src/store.ts` | Zustand，所有 `invoke()` 调用 |
| 修改 UI 组件 | `frontend/src/components/` | React 19 + shadcn/ui |
| 修改过滤逻辑 | `frontend/src-tauri/src/exif/stats.rs` | AND/OR 模式，7 维度过滤 |

## CONVENTIONS

### 前端（TypeScript/React）
- **无分号**：Prettier `semi: false`
- **单引号**：Prettier `singleQuote: true`
- **100 字符行宽**：Prettier `printWidth: 100`
- **无枚举**：tsconfig `erasableSyntaxOnly: true`（用 string union 代替）
- **无未使用变量**：tsconfig `noUnusedLocals: true`
- **路径别名**：`@/` → `./src/`
- **零警告**：ESLint `--max-warnings 0`

### 后端（Rust）
- **无自定义 clippy/rustfmt 配置**：使用默认值
- **错误类型**：统一 `Result<T, String>`
- **并行**：rayon，`MAX_IO_THREADS = 4`
- **文件删除**：`trash` crate（回收站，非永久删除）

## ANTI-PATTERNS

- **手动类型同步**：Rust 结构体 → TypeScript 接口需手动保持一致（`store.ts:6-70`）
- **CSP 已禁用**：`tauri.conf.json` 中 `csp: null`（生产环境需启用）
- **3 个未使用命令**：`delete_images`、`get_thumbnail`、`export_statistics` 已注册但前端未调用（`scan_images` 在 store 中有 action 但无组件调用）
- **前端无测试**：0 个测试文件，无测试框架配置

## COMMANDS

```bash
# 前端开发
cd frontend && npm run dev

# 前端构建
cd frontend && npm run build

# Rust 测试
cd frontend/src-tauri && cargo test

# 前端 lint
cd frontend && npm run lint

# 格式化
cd frontend && npm run format
```

## NOTES

- `src-tauri/` 在 `frontend/` 内部，不在项目根目录
- 全局状态用 `lazy_static!`（Rust）和 Zustand（TS）
- 缓存 DB 位于 `%APPDATA%/photo-exif-analyzer/exif_cache.db`
- 进度通过 `window.emit("scan_progress")` 事件发送
- 相机名称格式化（`make + model`）在 7 处重复，考虑提取
- 文件名提取（`path.split(/[/\\]/).pop()`）在 5 处重复，考虑提取
