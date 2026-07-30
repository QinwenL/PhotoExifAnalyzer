## ADDED Requirements

### Requirement: 选择文件夹
用户 SHALL 能够选择要扫描的文件夹。

#### Scenario: 打开文件夹选择器
- **WHEN** 用户点击"选择文件夹"按钮
- **THEN** 系统 SHALL 打开系统文件夹选择对话框

#### Scenario: 记住上次选择
- **WHEN** 用户选择文件夹后
- **AND** 下次打开应用
- **THEN** 系统 SHALL 自动加载上次的文件夹

### Requirement: 启动扫描
用户 SHALL 能够触发目录扫描。

#### Scenario: 点击扫描按钮
- **WHEN** 用户选择文件夹后点击"扫描"
- **THEN** 系统 SHALL 开始扫描目录
- **AND** 显示扫描进度

#### Scenario: 扫描进度显示
- **WHEN** 扫描进行中
- **THEN** 系统 SHALL 显示进度条
- **AND** 进度条 SHALL 显示已扫描/总数

### Requirement: 美观现代的界面
系统 SHALL 提供美观、现代的用户界面。

#### Scenario: 深色/浅色主题
- **WHEN** 用户打开应用
- **THEN** 系统 SHALL 默认使用深色主题
- **AND** 用户 SHALL 能够切换到浅色主题

#### Scenario: 响应式布局
- **WHEN** 用户调整窗口大小
- **THEN** 界面 SHALL 自适应调整布局

### Requirement: 快捷键支持
系统 SHALL 支持常用快捷键。

#### Scenario: 删除快捷键
- **WHEN** 用户选中图片后按 Delete 键
- **THEN** 系统 SHALL 触发删除流程

#### Scenario: 全选快捷键
- **WHEN** 用户按 Ctrl+A
- **THEN** 系统 SHALL 选中所有图片

### Requirement: 状态栏显示
系统 SHALL 在底部显示状态信息。

#### Scenario: 显示统计摘要
- **WHEN** 扫描完成
- **THEN** 状态栏 SHALL 显示：总图片数、选中数、当前筛选数
