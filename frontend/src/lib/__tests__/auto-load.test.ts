import { describe, expect, it } from 'vitest'
import { shouldAutoLoadLastDirectory, readLastDirectory } from '../auto-load'

/**
 * 验证"记住上次文件夹"决策逻辑。
 *
 * 规格 desktop-ui/spec.md：下次打开应用 SHALL 自动加载上次的文件夹。
 */
describe('shouldAutoLoadLastDirectory', () => {
  it('有 lastDirectory 时返回 true', () => {
    expect(shouldAutoLoadLastDirectory('/photos')).toBe(true)
  })

  it('lastDirectory 为 null 时返回 false', () => {
    expect(shouldAutoLoadLastDirectory(null)).toBe(false)
  })

  it('lastDirectory 为空字符串时返回 false', () => {
    expect(shouldAutoLoadLastDirectory('')).toBe(false)
  })
})

describe('readLastDirectory', () => {
  it('正常返回存储的目录', () => {
    const storage = { getItem: () => '/photos/2024' }
    expect(readLastDirectory(storage)).toBe('/photos/2024')
  })

  it('返回 null 时被规范化为 null', () => {
    const storage = { getItem: () => null }
    expect(readLastDirectory(storage)).toBeNull()
  })

  it('空字符串被规范化为 null', () => {
    const storage = { getItem: () => '' }
    expect(readLastDirectory(storage)).toBeNull()
  })

  it('storage 抛错时返回 null（不向上传播）', () => {
    const storage = {
      getItem: () => {
        throw new Error('SecurityError')
      },
    }
    expect(readLastDirectory(storage)).toBeNull()
  })
})
