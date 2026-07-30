## ADDED Requirements

### Requirement: 相机型号统计
系统 SHALL 统计并展示用户使用的相机型号及其拍摄数量。

#### Scenario: 显示相机使用频率
- **WHEN** 扫描完成
- **THEN** 系统 SHALL 显示所有相机型号列表
- **AND** 每个型号 SHALL 显示对应的图片数量
- **AND** 列表 SHALL 按使用频率降序排列

### Requirement: 镜头使用统计
系统 SHALL 统计并展示用户使用的镜头及其拍摄数量。

#### Scenario: 显示镜头使用频率
- **WHEN** 扫描完成
- **THEN** 系统 SHALL 显示所有镜头列表
- **AND** 每个镜头 SHALL 显示对应的图片数量

### Requirement: 焦距分布统计
系统 SHALL 统计焦距使用分布并以图表形式展示。

#### Scenario: 焦距直方图
- **WHEN** 扫描完成
- **THEN** 系统 SHALL 生成焦距分布直方图
- **AND** X 轴 SHALL 为焦距值（mm）
- **AND** Y 轴 SHALL 为对应图片数量

### Requirement: 按统计项筛选图片
用户 SHALL 能够点击统计项来筛选对应的图片。

#### Scenario: 点击相机型号筛选
- **WHEN** 用户点击某个相机型号
- **THEN** 图片列表 SHALL 只显示使用该相机拍摄的图片

### Requirement: 导出统计报告
用户 SHALL 能够将统计结果导出为 JSON 格式。

#### Scenario: 导出 JSON 报告
- **WHEN** 用户点击"导出统计"按钮
- **THEN** 系统 SHALL 生成包含所有统计数据的 JSON 文件
- **AND** 文件 SHALL 包含相机、镜头、焦距统计
