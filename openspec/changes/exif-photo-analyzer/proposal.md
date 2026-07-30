## Why

摄影师需要分析自己作品的 EXIF 数据来了解拍摄习惯（常用焦距、镜头、相机），同时需要一个高效的工具来管理和删除不满意的图片。现有的工具要么功能单一（只分析不管理），要么界面丑陋、性能差、资源占用高。

## What Changes

- **新建** Tauri 1.x 桌面应用，结合 Rust 后端 + React 前端
- **支持** 全部常见图片格式（JPG/PNG/TIFF/RAW 等）的 EXIF 读取
- **提供** 统计分析功能：焦距分布、镜头使用频率、相机型号统计、导出 JSON
- **实现** 图片浏览功能：缩略图网格 + 列表视图、详情查看、批量选择、排序
- **支持** 高级筛选：按相机/镜头/焦距/快门/光圈/ISO/日期组合筛选（AND/OR）
- **支持** 图片删除功能：移到回收站（可恢复）
- **记住** 上次打开的文件夹
- **界面** 美观流畅，资源占用低，性能优秀

## Capabilities

### New Capabilities

- `exif-parsing`: 读取和解析各种图片格式的 EXIF 元数据
- `statistics-analysis`: 统计焦距、镜头、相机等使用频率并可视化
- `image-gallery`: 图片缩略图浏览、详情查看、批量选择
- `file-management`: 图片删除（回收站）、文件操作
- `desktop-ui`: Tauri 桌面应用界面，美观流畅

### Modified Capabilities

（无，这是全新项目）

## Impact

- **技术栈**：Tauri 1.x + Rust + React + TypeScript
- **依赖**：
  - 后端：`kamadak-exif`（EXIF 解析）、`memmap2` + `memchr`（RAW 内嵌 JPEG preview 定位）、`walkdir`（目录遍历）、`trash`（回收站）、`rayon`（并行）、`rusqlite`（SQLite 缓存）
  - 前端：React、TailwindCSS、shadcn/ui、Recharts（图表）、@tanstack/react-virtual（虚拟滚动）
- **系统要求**：Windows/macOS/Linux，需要安装 Tauri 预依赖
- **性能目标**：10,000 张 JPEG < 5 秒完成扫描，RAW < 60 秒，界面 60fps 流畅
