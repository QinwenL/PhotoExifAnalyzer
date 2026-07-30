import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * P2.3: 验证 StatusBar 在批量删除时按规格显示进度条。
 *
 * Spec (file-management/spec.md "批量操作进度"):
 * - WHEN 用户删除超过 10 张图片
 * - THEN 系统 SHALL 显示进度条
 * - AND 进度条 SHALL 显示已完成数量/总数量
 *
 * 由于 vitest 配置为 node 环境（无 jsdom），这里采用与
 * confirm-dialog-preview.test.ts 相同的源码检查策略：断言
 * StatusBar.tsx 的源码包含规格要求的逻辑。
 */
describe('StatusBar delete progress bar (P2.3)', () => {
  const componentPath = join(__dirname, '..', 'StatusBar.tsx')
  const source = readFileSync(componentPath, 'utf-8')

  it('从 store 读取 deleteProcessed 和 deleteTotal', () => {
    // 没有这两个字段就无法显示 "N / M" 格式
    expect(source, 'StatusBar 应从 useAppStore 解构 deleteProcessed').toMatch(
      /\bdeleteProcessed\b/
    )
    expect(source, 'StatusBar 应从 useAppStore 解构 deleteTotal').toMatch(
      /\bdeleteTotal\b/
    )
  })

  it('仅在删除超过 10 张图片时显示进度条', () => {
    // spec: "WHEN 用户删除超过 10 张图片 THEN 系统 SHALL 显示进度条"
    // 进度条（visual bar）只应在 deleteTotal > 10 时渲染。
    // 用 \b10\b 确保匹配数字 10（而非 100、1000 等）。
    expect(
      source,
      'StatusBar 应包含 > 10 的阈值判断，仅在删除 >10 张时显示进度条'
    ).toMatch(/deleteTotal\s*>\s*10|10\s*<\s*deleteTotal/)
  })

  it('进度条显示 "已完成数量 / 总数量" 格式', () => {
    // spec: "进度条 SHALL 显示已完成数量/总数量"
    // 在 JSX 中是 `{deleteProcessed} / {deleteTotal}`，在模板字符串中是
    // `${deleteProcessed} / ${deleteTotal}`。两种都接受。
    expect(
      source,
      'StatusBar 应渲染 deleteProcessed / deleteTotal 格式的计数'
    ).toMatch(
      /\{deleteProcessed\}\s*\/\s*\{deleteTotal\}|\$\{deleteProcessed\}\s*\/\s*\$\{deleteTotal\}/
    )
  })

  it('渲染视觉进度条元素（非纯文本）', () => {
    // spec: "系统 SHALL 显示进度条" —— 需要一个视觉 bar 元素，
    // 而不是仅显示百分比文本。匹配 width 样式绑定的进度条 div。
    expect(
      source,
      'StatusBar 应渲染视觉进度条（width 样式绑定 deleteProgress）'
    ).toMatch(/width:\s*\$\{deleteProgress\}|width:\s*`?\$\{deleteProgress/)
  })
})
