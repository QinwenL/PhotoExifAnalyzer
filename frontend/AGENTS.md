# frontend/

Tauri 应用的前端层，包含 React UI 和 Rust 后端。

## STRUCTURE

```
frontend/
├── src-tauri/         # Rust 后端（见下方独立 AGENTS.md）
├── src/               # React 前端源码
│   ├── App.tsx        # 主应用壳，布局和路由
│   ├── store.ts       # Zustand 全局状态（所有 invoke 调用）
│   ├── main.tsx       # 入口，StrictMode
│   ├── components/    # UI 组件
│   ├── lib/           # 工具函数（cn() + Semaphore）
│   └── __tests__/     # Vitest 单元测试
├── index.html         # Vite 入口 HTML
├── package.json       # 依赖和脚本（test/test:watch via vitest）
├── vite.config.ts     # Vite 配置（@ 别名，端口 5173）
├── vitest.config.ts   # Vitest 配置
├── tsconfig.app.json  # TS 严格配置
├── eslint.config.js   # ESLint flat config
├── .prettierrc        # Prettier：无分号，单引号，100 宽
└── .oxlintrc.json     # OxLint（快速预提交检查）
```

## WHERE TO LOOK

| 任务 | 位置 | 备注 |
|------|------|------|
| 修改全局状态 | `src/store.ts` | 所有 `invoke()` 在这里 |
| 添加新组件 | `src/components/` | shadcn/ui 风格 |
| 修改图表 | `src/components/DistributionChart.tsx` 或 `FocalLengthChart.tsx` | Recharts |
| 修改虚拟滚动 | `src/components/VirtualizedGrid.tsx` | @tanstack/react-virtual |
| 修改筛选面板 | `src/components/FilterPanel.tsx` | 7 维度过滤 |
| 添加 shadcn 组件 | `components.json` 配置 + `src/components/ui/` | Radix + CVA |

## CONVENTIONS

- **组件文件**：PascalCase（`FilterPanel.tsx`）
- **工具函数**：camelCase（`lib/utils.ts`）
- **类型定义**：在 `store.ts` 顶部（手动镜像 Rust 类型，`ExportData` / `ExportImage` 含注释标注镜像来源）
- **导出**：`store.ts` 的 `exportToJSON` 委托后端 `export_statistics` 命令，逻辑只在 Rust 一处实现
- **测试**：Vitest，`npm run test`（run）/ `npm run test:watch`
- 格式化约定（无分号、单引号、100 宽）见根目录 AGENTS.md

## ANTI-PATTERNS

（无）
