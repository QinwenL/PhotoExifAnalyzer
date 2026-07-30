/**
 * 全局键盘快捷键行为决策。
 *
 * 将键盘事件映射为应用动作，便于单元测试与复用。
 * App.tsx 的 keydown 处理器调用此函数决定执行什么副作用。
 */

export type KeyboardAction =
  | { type: 'delete' }
  | { type: 'escape' }
  | { type: 'selectAll' }
  | { type: 'none' }

export interface KeyboardContext {
  /** 当前是否有选中的图片 */
  hasSelection: boolean
  /** 是否有详情面板打开 */
  hasDetailOpen: boolean
  /** 当前是否有可选择的图片结果 */
  hasResults: boolean
  /** 事件目标是否为文本输入元素（输入框/文本域），避免劫持浏览器文本选择 */
  isTextInput?: boolean
}

/**
 * 根据键盘事件和当前上下文，返回应当执行的动作。
 *
 * - Ctrl+A / Cmd+A：有结果且不在输入框时 → selectAll
 * - Delete：有选中且未打开详情时 → delete
 * - Escape：详情打开时 → escape
 * - 其他：none
 */
export function getKeyboardAction(
  event: { key: string; ctrlKey?: boolean; metaKey?: boolean },
  context: KeyboardContext
): KeyboardAction {
  // 文本输入场景下不劫持快捷键（让浏览器原生处理文本选择等）
  if (context.isTextInput) {
    return { type: 'none' }
  }

  // Escape：关闭详情面板（最高优先级，避免被其他逻辑拦截）
  if (event.key === 'Escape' && context.hasDetailOpen) {
    return { type: 'escape' }
  }

  // Ctrl+A / Cmd+A：全选
  if (
    (event.key === 'a' || event.key === 'A') &&
    (event.ctrlKey || event.metaKey) &&
    context.hasResults
  ) {
    return { type: 'selectAll' }
  }

  // Delete：删除选中（详情面板打开时不触发，避免误删）
  if (
    event.key === 'Delete' &&
    context.hasSelection &&
    !context.hasDetailOpen
  ) {
    return { type: 'delete' }
  }

  return { type: 'none' }
}

/**
 * 判断事件目标是否为文本输入元素。
 * 用于在 keydown 处理器中决定是否劫持快捷键。
 */
export function isTextInputTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  const tag = target.tagName.toLowerCase()
  if (tag === 'input') {
    const type = (target as HTMLInputElement).type.toLowerCase()
    return ['text', 'search', 'url', 'email', 'password', 'number', 'tel'].includes(type)
  }
  if (tag === 'textarea') return true
  return target.isContentEditable
}
