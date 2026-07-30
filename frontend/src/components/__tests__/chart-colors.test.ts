import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * 验证图表组件引用的 CSS 变量在 index.css 中有定义。
 *
 * 背景：早期版本使用 `hsl(var(--primary))` 引用变量，但 index.css
 * 使用 Tailwind v4 命名约定 `--color-primary`，导致变量解析失败、
 * 图表颜色回退到默认值，且不会跟随主题切换。
 */
describe('chart CSS variable references', () => {
  function extractCssVariables(cssContent: string): Set<string> {
    const variables = new Set<string>()
    // 匹配 --variable-name:
    const regex = /(--[a-z-]+)\s*:/g
    let match: RegExpExecArray | null
    while ((match = regex.exec(cssContent)) !== null) {
      variables.add(match[1])
    }
    return variables
  }

  function extractHslVarReferences(source: string): string[] {
    const refs: string[] = []
    // 匹配 hsl(var(--xxx))
    const regex = /hsl\(var\((--[a-z-]+)\)\)/g
    let match: RegExpExecArray | null
    while ((match = regex.exec(source)) !== null) {
      refs.push(match[1])
    }
    return refs
  }

  const cssPath = join(__dirname, '..', '..', 'index.css')
  const cssContent = readFileSync(cssPath, 'utf-8')
  const definedVars = extractCssVariables(cssContent)

  const chartFiles = [
    'DistributionChart.tsx',
    'FocalLengthChart.tsx',
  ]

  for (const file of chartFiles) {
    it(`${file} 中所有 hsl(var(--xxx)) 引用在 index.css 中有定义`, () => {
      const source = readFileSync(join(__dirname, '..', file), 'utf-8')
      const refs = extractHslVarReferences(source)
      expect(refs.length, '至少应有一个 hsl(var) 引用').toBeGreaterThan(0)
      for (const ref of refs) {
        expect(
          definedVars.has(ref),
          `${file} 引用未定义的 CSS 变量 ${ref}\n` +
            `index.css 中定义的变量: ${[...definedVars].join(', ')}`
        ).toBe(true)
      }
    })
  }
})
