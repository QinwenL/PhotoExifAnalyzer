import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/tauri', () => ({
  invoke: vi.fn(),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

import { useAppStore } from '../store'

describe('selectSingleImage', () => {
  beforeEach(() => {
    useAppStore.setState({
      selectedImages: new Set(),
      lastSelectedIndex: null,
    })
  })

  it('清空当前选择并仅选中给定路径', () => {
    useAppStore.setState({
      selectedImages: new Set(['/old1.jpg', '/old2.jpg']),
      lastSelectedIndex: 5,
    })

    useAppStore.getState().selectSingleImage('/target.jpg')

    const state = useAppStore.getState()
    expect(state.selectedImages).toEqual(new Set(['/target.jpg']))
  })

  it('重置 lastSelectedIndex 为 null', () => {
    useAppStore.setState({
      selectedImages: new Set(['/old.jpg']),
      lastSelectedIndex: 3,
    })

    useAppStore.getState().selectSingleImage('/target.jpg')

    expect(useAppStore.getState().lastSelectedIndex).toBeNull()
  })

  it('选中已选中的路径不会产生重复', () => {
    useAppStore.setState({
      selectedImages: new Set(['/target.jpg', '/other.jpg']),
    })

    useAppStore.getState().selectSingleImage('/target.jpg')

    expect(useAppStore.getState().selectedImages).toEqual(new Set(['/target.jpg']))
  })
})

describe('clearSelection', () => {
  beforeEach(() => {
    useAppStore.setState({
      selectedImages: new Set(),
      lastSelectedIndex: null,
    })
  })

  it('清空 selectedImages', () => {
    useAppStore.setState({
      selectedImages: new Set(['/a.jpg', '/b.jpg']),
    })

    useAppStore.getState().clearSelection()

    expect(useAppStore.getState().selectedImages.size).toBe(0)
  })

  it('重置 lastSelectedIndex 为 null（避免下次 Shift+Click 用陈旧索引）', () => {
    useAppStore.setState({
      selectedImages: new Set(['/a.jpg']),
      lastSelectedIndex: 7,
    })

    useAppStore.getState().clearSelection()

    expect(useAppStore.getState().lastSelectedIndex).toBeNull()
  })
})
