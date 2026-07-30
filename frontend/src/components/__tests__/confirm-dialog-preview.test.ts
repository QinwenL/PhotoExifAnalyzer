import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'

/**
 * 验证 ConfirmDialog 在删除确认弹窗中显示真实图片预览（Thumbnail 组件），
 * 而非仅以纯文本展示文件名。
 *
 * 背景：spec 要求"显示待删除图片预览"，但早期实现仅展示文件名文本，
 * 用户无法在确认前看到图片内容，容易误删。
 */
describe('ConfirmDialog image preview', () => {
  const componentPath = join(__dirname, '..', 'ConfirmDialog.tsx')
  const source = readFileSync(componentPath, 'utf-8')

  it('imports the Thumbnail component', () => {
    expect(
      source,
      'ConfirmDialog 应导入 Thumbnail 组件以显示真实图片预览'
    ).toMatch(/import\s+\{[^}]*\bThumbnail\b[^}]*\}\s+from\s+['"][^'"]+['"]/)
  })

  it('renders <Thumbnail path={...}> in the preview grid', () => {
    expect(
      source,
      'ConfirmDialog 应在预览网格中渲染 <Thumbnail path={img.path} />'
    ).toMatch(/<Thumbnail\s+path=\{img\.path\}/)
  })

  it('still surfaces the filename as title attribute for accessibility', () => {
    // 保留 title 属性以便鼠标悬停时显示完整文件名（无障碍/可访问性）
    expect(source).toMatch(/title=\{getFileName\(img\.path\)\}/)
  })
})
