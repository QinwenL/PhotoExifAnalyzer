# PhotoExifAnalyzer

桌面端照片 EXIF 元数据分析器。扫描目录、提取 EXIF、统计相机/镜头/焦段分布，支持过滤、删除、导出。

## 技术栈

- **后端**：Rust (edition 2021) + Tauri v1
- **前端**：React 19 + TypeScript + Vite 8
- **状态管理**：Zustand
- **UI 组件**：shadcn/ui + Tailwind CSS v4
- **图表**：Recharts
- **虚拟滚动**：@tanstack/react-virtual
- **EXIF 解析**：kamadak-exif
- **并行计算**：rayon（4 线程）
- **缓存**：SQLite (rusqlite)
- **文件删除**：trash crate（送回收站，非永久删除）

## 功能特性

- 扫描指定目录（支持递归），并行提取 EXIF 元数据
- 统计相机、镜头、焦段分布，以饼图/柱状图可视化
- 7 维度过滤（相机/镜头/焦距/光圈/ISO/快门/日期），支持 AND/OR 模式
- 网格/列表视图，虚拟滚动支持海量图片
- 单张/批量删除（移至回收站），删除后自动清理缓存
- 按名称/日期/大小/相机排序
- 导出统计结果为 JSON
- SQLite 缓存，重复扫描大幅减少磁盘 I/O
- 扫描进度实时反馈，可随时取消
- 浅色/深色/跟随系统主题切换

## 支持的图片格式

JPG/JPEG、TIFF、PNG、CR2/CR3（Canon）、NEF/NRW（Nikon）、ARW/SRF/SR2（Sony）、ORF（Olympus）、RAF（Fuji）、RW2（Panasonic）、PEF（Pentax）、DNG、RAW/RWL（Leica）、3FR（Hasselblad）、KDC/DCR（Kodak）、MRW（Minolta）、SRW（Samsung）、X3F（Sigma）、BAY（Casio）

> Windows 平台额外支持 HEIC 格式。

## 项目结构

```
PhotoExifAnalyzer/
├── frontend/
│   ├── src-tauri/             # Rust 后端
│   │   ├── src/
│   │   │   ├── lib.rs         # Tauri 命令层 + 全局状态
│   │   │   ├── main.rs        # 入口（Windows 子系统隐藏）
│   │   │   └── exif/          # 核心业务模块
│   │   │       ├── mod.rs     # ExifData 结构体（16 字段）
│   │   │       ├── parser.rs  # EXIF 提取
│   │   │       ├── scanner.rs # 目录扫描 + rayon 并行
│   │   │       ├── cache.rs   # SQLite 缓存
│   │   │       ├── stats.rs   # 统计 + 多维过滤
│   │   │       ├── file_ops.rs# 文件删除
│   │   │       ├── thumbnail.rs # 缩略图生成
│   │   │       └── heic.rs    # HEIC 支持（仅 Windows）
│   │   └── tests/
│   │       └── integration_test.rs
│   ├── src/                   # React 前端
│   │   ├── App.tsx            # 主应用壳
│   │   ├── store.ts           # Zustand 全局状态
│   │   ├── components/        # UI 组件
│   │   └── lib/               # 工具函数
│   └── package.json
└── openspec/                  # 规格文档
```

## 环境要求

- [Rust](https://www.rust-lang.org/) (stable)
- [Node.js](https://nodejs.org/) 18+
- [Tauri v1 前置依赖](https://v1.tauri.app/v1/guides/getting-started/prerequisites)

## 快速开始

```bash
# 进入前端目录
cd frontend

# 安装依赖
npm install

# 开发模式（启动 Tauri 开发窗口）
npm run tauri dev

# 构建生产版本
npm run tauri build
```

## 常用命令

```bash
# 前端开发服务器（仅 Web，无 Tauri）
cd frontend && npm run dev

# 前端构建
cd frontend && npm run build

# 前端 Lint
cd frontend && npm run lint

# 前端格式化
cd frontend && npm run format

# 前端测试
cd frontend && npm run test

# Rust 测试
cd frontend/src-tauri && cargo test
```

## 缓存与数据

- 缓存数据库位于：
  - Windows: `%APPDATA%/photo-exif-analyzer/exif_cache.db`
  - macOS: `~/Library/Application Support/photo-exif-analyzer/exif_cache.db`
  - Linux: `~/.local/share/photo-exif-analyzer/exif_cache.db`
- 缓存键为文件路径，校验依据为文件修改时间 + 缓存版本号
- 删除图片时自动清理对应的 EXIF 缓存与缩略图缓存

## 许可证

私有项目，未开源。
