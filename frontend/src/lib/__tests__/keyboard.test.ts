import { describe, expect, it } from 'vitest'
import { getKeyboardAction } from '../keyboard'

/**
 * 验证全局键盘快捷键的行为决策。
 *
 * 规格 desktop-ui/spec.md 要求：
 * - Ctrl+A 选中所有图片
 * - Delete 删除选中
 * - Escape 关闭详情
 */
describe('getKeyboardAction', () => {
  function makeEvent(
    key: string,
    opts: { ctrlKey?: boolean; metaKey?: boolean } = {}
  ): KeyboardEvent {
    return {
      key,
      ctrlKey: opts.ctrlKey ?? false,
      metaKey: opts.metaKey ?? false,
    } as KeyboardEvent
  }

  describe('Ctrl+A 全选', () => {
    it('Ctrl+A 在有结果时返回 selectAll', () => {
      const action = getKeyboardAction(
        makeEvent('a', { ctrlKey: true }),
        { hasSelection: false, hasDetailOpen: false, hasResults: true }
      )
      expect(action).toEqual({ type: 'selectAll' })
    })

    it('Cmd+A (macOS) 在有结果时返回 selectAll', () => {
      const action = getKeyboardAction(
        makeEvent('a', { metaKey: true }),
        { hasSelection: false, hasDetailOpen: false, hasResults: true }
      )
      expect(action).toEqual({ type: 'selectAll' })
    })

    it('Ctrl+A 在无结果时返回 none（无意义操作）', () => {
      const action = getKeyboardAction(
        makeEvent('a', { ctrlKey: true }),
        { hasSelection: false, hasDetailOpen: false, hasResults: false }
      )
      expect(action).toEqual({ type: 'none' })
    })

    it('Ctrl+A 大写 A 也生效（CapsLock/Shift）', () => {
      const action = getKeyboardAction(
        makeEvent('A', { ctrlKey: true }),
        { hasSelection: false, hasDetailOpen: false, hasResults: true }
      )
      expect(action).toEqual({ type: 'selectAll' })
    })

    it('无 Ctrl 修饰的 a 键不触发 selectAll', () => {
      const action = getKeyboardAction(
        makeEvent('a'),
        { hasSelection: false, hasDetailOpen: false, hasResults: true }
      )
      expect(action).toEqual({ type: 'none' })
    })
  })

  describe('Delete 删除', () => {
    it('Delete 在有选中且未打开详情时返回 delete', () => {
      const action = getKeyboardAction(
        makeEvent('Delete'),
        { hasSelection: true, hasDetailOpen: false, hasResults: true }
      )
      expect(action).toEqual({ type: 'delete' })
    })

    it('Delete 在无选中时返回 none', () => {
      const action = getKeyboardAction(
        makeEvent('Delete'),
        { hasSelection: false, hasDetailOpen: false, hasResults: true }
      )
      expect(action).toEqual({ type: 'none' })
    })

    it('Delete 在详情打开时返回 none（避免误删）', () => {
      const action = getKeyboardAction(
        makeEvent('Delete'),
        { hasSelection: true, hasDetailOpen: true, hasResults: true }
      )
      expect(action).toEqual({ type: 'none' })
    })
  })

  describe('Escape 关闭详情', () => {
    it('Escape 在详情打开时返回 escape', () => {
      const action = getKeyboardAction(
        makeEvent('Escape'),
        { hasSelection: false, hasDetailOpen: true, hasResults: true }
      )
      expect(action).toEqual({ type: 'escape' })
    })

    it('Escape 在无详情打开时返回 none', () => {
      const action = getKeyboardAction(
        makeEvent('Escape'),
        { hasSelection: false, hasDetailOpen: false, hasResults: true }
      )
      expect(action).toEqual({ type: 'none' })
    })
  })

  describe('输入框场景', () => {
    it('在输入框中按 Ctrl+A 不触发全选（让浏览器处理文本选择）', () => {
      const action = getKeyboardAction(
        makeEvent('a', { ctrlKey: true }),
        {
          hasSelection: false,
          hasDetailOpen: false,
          hasResults: true,
          isTextInput: true,
        }
      )
      expect(action).toEqual({ type: 'none' })
    })
  })
})
