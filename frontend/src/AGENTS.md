# src/

React 19 前端，使用 Zustand 状态管理 + shadcn/ui 组件库。

## STRUCTURE

```
src/
├── App.tsx              # 主应用壳，布局和路由
├── store.ts             # Zustand 全局状态（所有 invoke 调用）
├── main.tsx             # 入口，StrictMode
├── App.css              # 全局样式
├── index.css            # Tailwind 入口
├── components/
│   ├── ui/              # shadcn/ui 基础组件
│   │   └── button.tsx
│   ├── VirtualizedGrid.tsx   # 虚拟滚动网格/列表
│   ├── Thumbnail.tsx         # 单张图片缩略图
│   ├── ImageDetail.tsx       # 图片详情模态框
│   ├── FilterPanel.tsx       # 筛选面板（7 维度）
│   ├── DistributionChart.tsx # 相机/镜头饼图
│   ├── FocalLengthChart.tsx  # 焦段柱状图
│   ├── StatusBar.tsx         # 底部状态栏
│   └── ConfirmDialog.tsx     # 删除确认对话框
├── hooks/               # 自定义 hooks（空）
└── lib/
    └── utils.ts         # cn() 工具函数
```

## WHERE TO LOOK

| 任务 | 位置 | 备注 |
|------|------|------|
| 修改全局状态 | `store.ts` | 所有 `invoke()` 在这里 |
| 添加/修改组件 | `components/` | PascalCase 命名 |
| 修改图表 | `DistributionChart.tsx` / `FocalLengthChart.tsx` | Recharts |
| 修改虚拟滚动 | `VirtualizedGrid.tsx` | @tanstack/react-virtual |
| 修改筛选 | `FilterPanel.tsx` | AND/OR 模式 |
| 添加 shadcn 组件 | `components/ui/` | Radix + CVA |

## CONVENTIONS

- **无分号**、**单引号**、**100 字符行宽**
- **路径别名**：`@/` → `./src/`
- **组件**：函数组件 + hooks，无 class 组件
- **状态**：Zustand `useAppStore`，单一 store
- **Tauri 通信**：`invoke()` 调用命令，`listen()` 接收事件
- **持久化**：`localStorage` 存 `lastDirectory` 和 `theme`

## ANTI-PATTERNS

- **错误静默**：`.catch(error => console.error(...))` 不向用户展示
