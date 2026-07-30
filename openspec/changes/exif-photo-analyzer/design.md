## Context

这是一个全新的桌面应用项目，用于分析图片 EXIF 信息并管理图片。用户是摄影师，需要：
- 快速扫描大量图片（可能上万张）
- 分析拍摄习惯（焦距、镜头、相机）
- 浏览和管理图片（删除到回收站）

现有方案的问题：
- Lightroom 等专业软件太重，启动慢
- 简单的 EXIF 查看器没有统计功能
- Web 应用无法直接操作本地文件系统

## Goals / Non-Goals

**Goals:**
- 极低资源占用（< 100MB 内存，< 1% CPU 空闲时）
- 快速扫描（10,000 张 JPEG < 5 秒，RAW < 5 分钟）
- 美观现代的界面
- 跨平台（Windows/macOS/Linux）
- 支持所有常见图片格式

**Non-Goals:**
- 不做图片编辑功能
- 不做云端同步
- 不做复杂的标签系统
- 不做 AI 图片识别
- 不做地理位置地图视图（Phase 2）

## Decisions

### 1. 选择 Tauri 而非 Electron

**决策**：使用 Tauri 1.x（稳定版）

**理由**：
- 包体积：Tauri ~5MB vs Electron ~150MB
- 内存占用：Tauri ~30MB vs Electron ~150MB+
- 性能：Rust 后端原生性能，JS 只做 UI
- 安全：更小的攻击面，更好的权限控制
- 稳定性：1.x 已成熟，文档完整，生态丰富

**替代方案考虑**：
- Electron：生态成熟但资源占用过高
- Tauri 2.x：新特性但 Beta 阶段，个人工具用 1.x 更稳妥
- Flutter Desktop：Rust 生态不如 Tauri
- 原生开发：跨平台成本太高

### 2. Rust 后端架构

**决策**：使用 Tauri commands 作为前后端通信层

```rust
// 核心模块
src-tauri/
├── src/
│   ├── main.rs          // Tauri 入口（Windows 子系统隐藏）
│   ├── lib.rs           // 全部 Tauri commands + 全局状态 + 单元测试
│   └── exif/            // EXIF 解析与相关逻辑
│       ├── mod.rs       // ExifData 结构体 + camera_name()
│       ├── parser.rs    // JPEG/TIFF/RAW EXIF 解析（kamadak-exif）
│       ├── raw_scan.rs  // RAW 内嵌 JPEG preview 定位（memmap2 + memchr）
│       ├── scanner.rs   // 目录扫描器（walkdir + rayon 并行）
│       ├── stats.rs     // 统计计算 + AND/OR 过滤
│       ├── cache.rs     // SQLite EXIF 缓存
│       ├── thumbnail.rs // 缩略图生成
│       ├── file_ops.rs  // 回收站删除
│       └── heic.rs      // HEIC 支持（仅 Windows）
```

**核心依赖**：
- `kamadak-exif`：JPEG/TIFF EXIF 解析（同时用于 RAW 内嵌 JPEG preview 的 EXIF 提取）
- `memmap2` + `memchr`：RAW 文件零拷贝映射 + SIMD 加速的嵌入式 JPEG preview 定位（替代 `rawloader`，后者只解码像素不提取 EXIF）
- `walkdir`：目录遍历
- `rayon`：并行扫描
- `trash`：回收站操作
- `serde` + `serde_json`：JSON 序列化

**理由**：
- Rust 处理大量文件 I/O 和 EXIF 解析非常高效
- Tauri commands 是类型安全的 RPC
- 可以异步处理不阻塞 UI

### 3. 前端架构

**决策**：React + TypeScript + TailwindCSS + shadcn/ui

**理由**：
- React 生态成熟，组件库丰富
- TailwindCSS 快速开发美观界面
- shadcn/ui：现代设计、按需复制组件、无冗余代码
- TypeScript 类型安全

**shadcn/ui 优势**：
- 基于 Radix UI，无障碍性好
- 组件直接复制到项目，完全可控
- 与 TailwindCSS 深度集成

**UI 设计**：
```
┌─────────────────────────────────────────┐
│  [选择文件夹]  [扫描]  [统计]  [设置]     │
├──────────┬──────────────────────────────┤
│ 统计面板 │                              │
│ ──────── │      图片网格视图            │
│ 相机:    │                              │
│ - A7R4   │   ┌────┐ ┌────┐ ┌────┐     │
│ - A7M3   │   │    │ │    │ │    │     │
│          │   └────┘ └────┘ └────┘     │
│ 镜头:    │                              │
│ - 24-70  │   ┌────┐ ┌────┐ ┌────┐     │
│ - 85 f1.4│   │    │ │    │ │    │     │
│          │   └────┘ └────┘ └────┘     │
│ 焦距:    │                              │
│ ████████ │   [全选] [删除] [导出]       │
└──────────┴──────────────────────────────┘
```

### 4. EXIF 解析策略

**决策**：使用 `kamadak-exif` + 并行处理

**关键实现**：
- 首次扫描：读取基础 EXIF（快）
- 详情查看：读取完整 EXIF（按需）
- 并行扫描：使用 `rayon` 多线程

**缩略图策略（混合方案）**：
- 网格浏览：生成小缩略图缓存（~150px），保证流畅
- 详情查看：实时解码原图，保证清晰
- 缩略图存储：应用数据目录（非图片目录）
  - Windows: `%APPDATA%/PhotoExifAnalyzer/thumbnails/`
  - macOS: `~/Library/Application Support/PhotoExifAnalyzer/thumbnails/`
  - Linux: `~/.local/share/PhotoExifAnalyzer/thumbnails/`
- 文件命名：使用 xxhash(原文件路径) 避免路径问题

**EXIF 缓存策略**：
- 存储方式：SQLite（索引查询、原子事务、并发安全）
- 缓存位置：应用数据目录
- 更新机制：检查文件修改时间，变化则重新解析
- 智能清理：启动时删除不存在文件的缓存条目

### 5. 图片格式支持

**决策**：支持主流格式，包括 RAW

| 格式 | 支持方式 | 优先级 |
|------|----------|--------|
| JPEG | kamadak-exif 原生 | P0 |
| TIFF | kamadak-exif 原生 | P0 |
| RAW (CR2/NEF/ARW等) | 定位内嵌 JPEG preview 后用 kamadak-exif 解析 | P0 |
| PNG | 部分 EXIF 支持 | P1 |
| HEIC/HEIF | 需要额外库（Windows 平台已实现） | P2 |

### 6. 删除功能实现

**决策**：使用系统回收站

**实现**：
```rust
// 跨平台回收站
trash::delete(&file_path)?;
```

**安全措施**：
- 删除前显示图片预览
- 批量删除需要二次确认
- 显示将要删除的文件数量

## 架构设计

### 状态管理（React）

使用 **Zustand** 管理全局状态，简单轻量：

```typescript
// 核心 Store
interface ScanStore {
  folder: string | null;
  images: ImageMeta[];
  scanProgress: number;
  isScanning: boolean;
  error: string | null;
}

interface FilterStore {
  criteria: FilterCriteria[];
  logic: 'AND' | 'OR';
  filteredImages: ImageMeta[];
}

interface UIStore {
  viewMode: 'grid' | 'list';
  selectedIds: Set<string>;
  detailPanelOpen: boolean;
  theme: 'dark' | 'light';
}
```

### 缩略图传递机制

使用 **Tauri asset protocol**：

1. 后端生成缩略图 → 存储到应用数据目录
2. 注册 `asset://` 协议 → 前端直接访问
3. 前端使用 `<img src="asset://localhost/thumbnails/xxx.jpg">`

优势：零拷贝、无 Base64 开销、安全

### 缓存系统

使用 **SQLite**（`rusqlite`）替代 JSON：

```sql
-- EXIF 缓存表
CREATE TABLE exif_cache (
  path TEXT PRIMARY KEY,
  modified_time INTEGER NOT NULL,
  exif_json TEXT NOT NULL,           -- JSON 格式的 EXIF
  version INTEGER NOT NULL,          -- 缓存版本号，schema 变化时自动重建
  preview_offset INTEGER,            -- RAW 内嵌 JPEG preview 字节偏移
  preview_length INTEGER             -- RAW 内嵌 JPEG preview 字节长度
);

CREATE INDEX idx_modified_time ON exif_cache(modified_time);
CREATE INDEX idx_version ON exif_cache(version);

-- 统计缓存表（可选，未实现）
-- 统计数据由 stats.rs 从 ScanResult 实时计算（毫秒级），无需额外缓存。
-- CREATE TABLE stats_cache (
--   folder TEXT PRIMARY KEY,
--   stats_data TEXT,
--   updated_at INTEGER
-- );
```

优势：索引查询、原子事务、并发安全

### 扫描策略

采用 **两阶段扫描**：

```
阶段1：快速扫描（<1秒）
  ├── 遍历目录，获取文件列表
  ├── 检查缓存有效性
  └── 立即返回结果，UI 可用

阶段2：深度扫描（后台）
  ├── 解析 EXIF 数据
  ├── 生成缩略图
  └── 更新 UI（渐进式）
```

### 取消机制

使用 `tokio::sync::watch` 实现取消：

```rust
let (cancel_tx, cancel_rx) = watch::channel(false);

// 扫描任务检查
if *cancel_rx.borrow() {
  return Err(ScanError::Cancelled);
}
```

前端：扫描时显示"取消"按钮

### 错误处理策略

| 错误类型 | 处理方式 | 用户提示 |
|----------|----------|----------|
| 单文件 EXIF 解析失败 | 跳过，记录日志 | "X 张图片解析失败" |
| 权限错误 | 跳过文件夹 | "无法访问 XXX" |
| 缓存损坏 | 自动重建 | 无感 |
| 磁盘空间不足 | 停止缩略图生成 | "磁盘空间不足" |
| 扫描取消 | 保存部分结果 | 无感 |

## 开发流程

**核心原则**：TDD 模式，小步快跑，禁止一次性开发大量代码

**开发节奏**：
```
1. 写测试 → 验证失败（红）
2. 写最小实现 → 验证通过（绿）
3. 重构优化 → 保持测试通过
4. 提交代码 → 下一个任务
```

**任务拆分原则**：
- 每个任务可在 1-2 小时内完成
- 每个任务有明确的验证标准
- 任务之间尽量独立，减少依赖
- 先实现核心功能，再添加优化

**模块化约束**：
- 每个模块职责单一
- 模块间通过接口通信
- 修改一个模块不影响其他模块
- 每个模块独立可测试

**验证方式**：
- 单元测试：核心逻辑
- 集成测试：模块间交互
- 手动测试：UI 交互

## Risks / Trade-offs

### 风险 1: RAW 格式兼容性
**风险**：不同相机厂商 RAW 格式差异大，嵌入式 JPEG preview 位置/大小不一致
**缓解**：用 `memmap2` + `memchr` 扫描 SOI/EOI 标记定位 preview，覆盖 Canon/Nikon/Sony 主流厂商；优先扫描前后 8MB 区间，未命中回退全文件扫描保证不漏

### 风险 2: 大量图片内存占用
**风险**：10,000 张图片的缩略图可能占用较多内存
**缓解**：
- 虚拟滚动（只渲染可见区域）
- 缩略图按需加载
- 限制同时加载数量

### 风险 3: 跨平台文件系统差异
**风险**：Windows/macOS/Linux 路径和回收站行为不同
**缓解**：使用 `trash` crate 封装，Tauri 已处理路径问题

### 权衡: 功能 vs 复杂度
为了快速交付，第一版不包含：
- 图片编辑
- 云同步
- AI 分类
这些可以作为后续版本功能。

## Migration Plan

这是全新项目，无需迁移。

**部署步骤**：
1. 开发环境：`cargo tauri dev`
2. 构建发布版：`cargo tauri build`
3. 分发：提供安装包（Windows .msi/.exe，macOS .dmg，Linux .AppImage）

**回滚策略**：
- 开发阶段：Git 版本控制
- 发布后：提供旧版本下载链接

## Open Questions

（已全部决策完成）

1. ~~RAW 格式支持~~ → 确认支持，P0 优先级
2. ~~缩略图策略~~ → 混合方案（小图缓存 + 详情实时解码）
3. ~~缓存存储~~ → SQLite（`rusqlite`，索引查询、原子事务、并发安全）
4. ~~Tauri 版本~~ → 1.x 稳定版
5. ~~UI 库~~ → shadcn/ui
