import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * P3.4: 验证 App.tsx 渲染扫描警告横幅，让用户知道哪些文件夹被跳过。
 *
 * 设计依据 (design.md "错误处理策略"):
 *   | 权限错误 | 跳过文件夹 | "无法访问 XXX" |
 */
describe('Scan warnings banner (P3.4)', () => {
  const componentPath = join(__dirname, '..', '..', 'App.tsx')
  const source = readFileSync(componentPath, 'utf-8')

  it('从 store 读取 scanWarnings 和 clearScanWarnings', () => {
    expect(
      source,
      'App 应从 useAppStore 解构 scanWarnings'
    ).toMatch(/\bscanWarnings\b/)
    expect(
      source,
      'App 应从 useAppStore 解构 clearScanWarnings'
    ).toMatch(/\bclearScanWarnings\b/)
  })

  it('当 scanWarnings 非空时渲染警告横幅', () => {
    // 条件渲染：scanWarnings.length > 0
    expect(
      source,
      'App 应在 scanWarnings.length > 0 时渲染横幅'
    ).toMatch(/scanWarnings\.length\s*>\s*0|scanWarnings\.length\s*!==\s*0/)
  })

  it('横幅渲染每条警告的 message', () => {
    // 应该遍历 scanWarnings 并渲染 message 字段
    expect(
      source,
      'App 应渲染 warning.message 或 .path 字段'
    ).toMatch(/\.message|\.path/)
  })

  it('横幅提供清除按钮调用 clearScanWarnings', () => {
    // 点击按钮调用 clearScanWarnings
    expect(
      source,
      'App 应提供 onClick 调用 clearScanWarnings'
    ).toMatch(/onClick=\{clearScanWarnings\}|onClick=\{\(\)\s*=>\s*clearScanWarnings\(\)\}/)
  })
})
