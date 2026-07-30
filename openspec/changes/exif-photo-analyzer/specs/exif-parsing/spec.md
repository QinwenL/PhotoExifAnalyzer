## ADDED Requirements

### Requirement: 解析 JPEG 图片 EXIF 数据
系统 SHALL 能够读取和解析 JPEG 格式图片的 EXIF 元数据，包括但不限于：相机型号、镜头信息、焦距、光圈、快门速度、ISO、拍摄时间。

#### Scenario: 解析标准 JPEG EXIF
- **WHEN** 用户选择包含 JPEG 图片的文件夹
- **THEN** 系统 SHALL 在 100ms 内完成单张图片 EXIF 解析
- **AND** 返回包含所有可用 EXIF 字段的结构化数据

#### Scenario: 处理缺失 EXIF 的图片
- **WHEN** JPEG 图片不包含 EXIF 数据
- **THEN** 系统 SHALL 返回空的 EXIF 对象
- **AND** 不抛出错误

### Requirement: 解析 TIFF 图片 EXIF 数据
系统 SHALL 能够读取 TIFF 格式图片的 EXIF 元数据。

#### Scenario: 解析 TIFF EXIF
- **WHEN** 用户选择包含 TIFF 图片的文件夹
- **THEN** 系统 SHALL 正确解析 TIFF 文件的 EXIF 信息

### Requirement: 支持 RAW 格式
系统 SHALL 支持读取主流相机 RAW 格式（CR2/NEF/ARW）的 EXIF 数据。

#### Scenario: 解析 Canon RAW
- **WHEN** 用户选择包含 .CR2 文件的文件夹
- **THEN** 系统 SHALL 提取嵌入的 EXIF 信息

### Requirement: 并行扫描目录
系统 SHALL 使用多线程并行扫描目录中的图片文件。

#### Scenario: 扫描大量图片
- **WHEN** 用户选择包含 10,000 张图片的文件夹
- **THEN** 扫描 SHALL 在 5 秒内完成
- **AND** CPU 使用率 SHALL 不超过 80%

### Requirement: 异步扫描不阻塞 UI
系统 SHALL 以异步方式执行扫描命令，扫描期间前端 UI SHALL 保持响应。

#### Scenario: 扫描期间界面可交互
- **WHEN** 用户触发扫描且扫描进行中
- **THEN** 扫描命令 SHALL 在后台线程执行（`spawn_blocking`）
- **AND** Tauri 主线程 SHALL 不被阻塞
- **AND** 前端 SHALL 能够接收扫描进度事件并更新界面

#### Scenario: 扫描取消
- **WHEN** 用户在扫描进行中点击取消
- **THEN** 系统 SHALL 通过共享取消标志通知后台扫描线程
- **AND** 扫描线程 SHALL 在检查到取消标志后尽快停止
- **AND** 已扫描的部分结果 SHALL 被丢弃（返回空列表）

### Requirement: 扫描进度实时推送
系统 SHALL 在扫描过程中通过 Tauri 事件实时推送扫描进度。

#### Scenario: 进度事件推送
- **WHEN** 扫描器处理图片文件时
- **THEN** 系统 SHALL 通过 `scan_progress` 事件推送进度百分比（0-100）
- **AND** 进度百分比 SHALL 等于 `已处理文件数 / 总文件数 * 100`
- **AND** 事件 SHALL 在后台线程中发送（`window.emit`）

#### Scenario: 进度从 0 开始
- **WHEN** 扫描开始时
- **THEN** 前端 SHALL 显示进度为 0%
- **AND** 扫描完成后 SHALL 显示进度为 100%

### Requirement: 缓存 EXIF 数据
系统 SHALL 将解析的 EXIF 数据缓存到本地数据库。

#### Scenario: 二次扫描加速
- **WHEN** 用户再次扫描同一文件夹
- **THEN** 系统 SHALL 使用缓存数据
- **AND** 扫描时间 SHALL 减少 80% 以上

#### Scenario: 文件修改后重新解析
- **WHEN** 图片文件的修改时间发生变化
- **THEN** 系统 SHALL 重新解析该文件的 EXIF
