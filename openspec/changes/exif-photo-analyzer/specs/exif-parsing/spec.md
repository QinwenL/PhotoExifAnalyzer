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

### Requirement: 缓存 EXIF 数据
系统 SHALL 将解析的 EXIF 数据缓存到本地数据库。

#### Scenario: 二次扫描加速
- **WHEN** 用户再次扫描同一文件夹
- **THEN** 系统 SHALL 使用缓存数据
- **AND** 扫描时间 SHALL 减少 80% 以上

#### Scenario: 文件修改后重新解析
- **WHEN** 图片文件的修改时间发生变化
- **THEN** 系统 SHALL 重新解析该文件的 EXIF
