import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/tauri', () => ({
  invoke: vi.fn(),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

import { invoke } from '@tauri-apps/api/tauri'
import { useAppStore } from '../store'

/**
 * 验证原本静默的错误（只 console.error）现在会写入 errorMessage 状态，
 * 让 UI 能向用户反馈失败原因。
 */
describe('error reporting to UI', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useAppStore.setState({
      errorMessage: null,
      isScanning: false,
      scanProgress: 0,
      scanResults: [],
      filteredResults: [],
      selectedImages: new Set(),
      lastSelectedIndex: null,
      isDeleting: false,
      deleteProgress: 0,
    })
  })

  describe('scanDirectoryWithProgress', () => {
    it('扫描失败时设置 errorMessage', async () => {
      vi.mocked(invoke).mockRejectedValue(new Error('Permission denied'))

      await useAppStore.getState().scanDirectoryWithProgress('/restricted', true)

      const state = useAppStore.getState()
      expect(state.isScanning).toBe(false)
      expect(state.errorMessage).toContain('扫描失败')
      expect(state.errorMessage).toContain('Permission denied')
    })

    it('扫描成功时 errorMessage 保持为 null', async () => {
      vi.mocked(invoke).mockResolvedValue([])

      await useAppStore.getState().scanDirectoryWithProgress('/photos', true)

      expect(useAppStore.getState().errorMessage).toBeNull()
    })
  })

  describe('cancelScan', () => {
    it('取消扫描失败时设置 errorMessage', async () => {
      vi.mocked(invoke).mockRejectedValue(new Error('cancel failed'))

      await useAppStore.getState().cancelScan()

      expect(useAppStore.getState().errorMessage).toContain('取消扫描失败')
    })
  })

  describe('clearErrorMessage', () => {
    it('清空 errorMessage 状态', () => {
      useAppStore.setState({ errorMessage: 'some error' })
      useAppStore.getState().clearErrorMessage()
      expect(useAppStore.getState().errorMessage).toBeNull()
    })
  })
})
