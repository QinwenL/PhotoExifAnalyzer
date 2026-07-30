## ADDED Requirements

### Requirement: 删除图片到回收站
用户 SHALL 能够将选中的图片移到系统回收站。

#### Scenario: 单张删除
- **WHEN** 用户选中一张图片并点击删除
- **THEN** 系统 SHALL 显示确认对话框
- **AND** 对话框 SHALL 显示图片预览和文件名
- **AND** 用户确认后 SHALL 移到回收站

#### Scenario: 批量删除
- **WHEN** 用户选中多张图片并点击删除
- **THEN** 系统 SHALL 显示确认对话框
- **AND** 对话框 SHALL 显示将删除的图片数量
- **AND** 用户确认后 SHALL 全部移到回收站

#### Scenario: 取消删除
- **WHEN** 用户在确认对话框点击取消
- **THEN** 图片 SHALL 保持不变

### Requirement: 删除确认机制
系统 SHALL 要求用户确认删除操作。

#### Scenario: 二次确认
- **WHEN** 用户点击删除按钮
- **THEN** 系统 SHALL 弹出确认对话框
- **AND** 对话框 SHALL 包含"确认"和"取消"按钮

### Requirement: 批量操作进度
系统 SHALL 显示批量操作的进度。

#### Scenario: 显示进度条
- **WHEN** 用户删除超过 10 张图片
- **THEN** 系统 SHALL 显示进度条
- **AND** 进度条 SHALL 显示已完成数量/总数量
