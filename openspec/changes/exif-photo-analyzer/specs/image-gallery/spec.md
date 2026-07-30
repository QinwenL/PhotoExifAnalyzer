## ADDED Requirements

### Requirement: 缩略图网格显示
系统 SHALL 以网格形式显示图片缩略图。

#### Scenario: 加载缩略图
- **WHEN** 扫描完成
- **THEN** 系统 SHALL 显示图片缩略图网格
- **AND** 缩略图 SHALL 保持原始宽高比
- **AND** 每个缩略图 SHALL 显示文件名

#### Scenario: 虚拟滚动
- **WHEN** 图片数量超过 100 张
- **THEN** 系统 SHALL 使用虚拟滚动
- **AND** 滚动 SHALL 流畅无卡顿

### Requirement: 图片详情查看
用户 SHALL 能够查看图片的详细 EXIF 信息。

#### Scenario: 打开详情面板
- **WHEN** 用户双击某张图片
- **THEN** 系统 SHALL 显示详情面板
- **AND** 面板 SHALL 包含：大图预览、完整 EXIF 信息

### Requirement: 批量选择图片
用户 SHALL 能够批量选择多张图片。

#### Scenario: 点击选择
- **WHEN** 用户单击图片
- **THEN** 该图片 SHALL 被选中（高亮显示）

#### Scenario: Shift 多选
- **WHEN** 用户按住 Shift 键点击图片
- **THEN** 系统 SHALL 选中两张图片之间的所有图片

#### Scenario: Ctrl 多选
- **WHEN** 用户按住 Ctrl 键点击图片
- **THEN** 系统 SHALL 切换该图片的选中状态

#### Scenario: 全选
- **WHEN** 用户点击"全选"按钮或按 Ctrl+A
- **THEN** 系统 SHALL 选中当前筛选结果中的所有图片

### Requirement: 排序功能
用户 SHALL 能够按不同字段对图片排序。

#### Scenario: 按日期排序
- **WHEN** 用户选择按拍摄日期排序
- **THEN** 图片 SHALL 按拍摄时间升序或降序排列

#### Scenario: 按文件名排序
- **WHEN** 用户选择按文件名排序
- **THEN** 图片 SHALL 按文件名字母顺序排列
