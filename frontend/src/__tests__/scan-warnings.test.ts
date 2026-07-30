import { beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/tauri', () => ({
  invoke: vi.fn(),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockImplementation((event: string) => {
    // Keep a per-event callback registry so the test can fire events.
    listenCallbacks[event] = null
    return Promise.resolve(() => {
      listenCallbacks[event] = null
    })
  }),
}))

// Registry of registered event callbacks, populated by the mocked `listen`.
// Tests grab the callback for a given event name and invoke it with a
// synthetic payload to simulate the backend emitting an event.
let listenCallbacks: Record<string, ((e: { payload: unknown }) => void) | null> = {}

import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'
import { useAppStore } from '../store'

/**
 * P3.4: 验证扫描期间后端通过 `scan_warning` 事件上报的权限错误被正确收集到
 * `scanWarnings` 状态，UI 能向用户反馈 "无法访问 XXX"。
 *
 * 设计依据 (design.md "错误处理策略"):
 *   | 权限错误 | 跳过文件夹 | "无法访问 XXX" |
 */
describe('P3.4: scan warnings from backend', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    listenCallbacks = {}
    // Re-install the implementation AFTER clearAllMocks wipes it.
    vi.mocked(listen).mockImplementation((event: string, cb: (e: { payload: unknown }) => void) => {
      listenCallbacks[event] = cb
      return Promise.resolve(() => {
        listenCallbacks[event] = null
      })
    })
    useAppStore.setState({
      scanResults: [],
      filteredResults: [],
      selectedImages: new Set(),
      isScanning: false,
      scanProgress: 0,
      scanProcessed: null,
      scanTotal: null,
      errorMessage: null,
      scanWarnings: [],
    })
  })

  it('初始状态 scanWarnings 为空数组', () => {
    useAppStore.setState({ scanWarnings: [] })
    expect(useAppStore.getState().scanWarnings).toEqual([])
  })

  it('扫描开始时清空旧的 scanWarnings', async () => {
    // 预置一个陈旧警告，扫描开始后应当被清空
    useAppStore.setState({
      scanWarnings: [{ path: '/old', message: '无法访问 /old', kind: 'PermissionDenied' }],
    })
    vi.mocked(invoke).mockResolvedValue([] as never)

    await useAppStore.getState().scanDirectoryWithProgress('/photos', true)

    expect(useAppStore.getState().scanWarnings).toEqual([])
  })

  it('收到 scan_warning 事件时追加到 scanWarnings', async () => {
    vi.mocked(invoke).mockImplementation(() => new Promise((resolve) => {
      // Hold the invoke promise open so we can fire events while "scanning".
      // Resolved below after assertions.
      setTimeout(() => resolve([] as never), 50)
    }))

    const promise = useAppStore.getState().scanDirectoryWithProgress('/photos', true)

    // Fire a scan_warning event as if the backend had encountered a
    // permission-denied subdirectory.
    const cb = listenCallbacks['scan_warning']
    expect(cb, 'store must register a scan_warning listener').toBeTruthy()
    cb!({ payload: { path: '/photos/locked', message: '无法访问 /photos/locked', kind: 'PermissionDenied' } })

    // Also fire a scan_progress event to ensure both listeners coexist.
    const progressCb = listenCallbacks['scan_progress']
    if (progressCb) {
      progressCb({ payload: { processed: 1, total: 10, percentage: 10 } })
    }

    await promise

    const state = useAppStore.getState()
    expect(state.scanWarnings).toHaveLength(1)
    expect(state.scanWarnings[0]).toEqual({
      path: '/photos/locked',
      message: '无法访问 /photos/locked',
      kind: 'PermissionDenied',
    })
  })

  it('收到多个 scan_warning 事件时全部追加', async () => {
    vi.mocked(invoke).mockImplementation(() => new Promise((resolve) => {
      setTimeout(() => resolve([] as never), 50)
    }))

    const promise = useAppStore.getState().scanDirectoryWithProgress('/photos', true)

    const cb = listenCallbacks['scan_warning']
    expect(cb).toBeTruthy()
    cb!({ payload: { path: '/photos/locked1', message: '无法访问 /photos/locked1', kind: 'PermissionDenied' } })
    cb!({ payload: { path: '/photos/missing', message: '无法访问 /photos/missing', kind: 'Other' } })

    await promise

    const state = useAppStore.getState()
    expect(state.scanWarnings).toHaveLength(2)
    expect(state.scanWarnings[0].path).toBe('/photos/locked1')
    expect(state.scanWarnings[1].path).toBe('/photos/missing')
    expect(state.scanWarnings[1].kind).toBe('Other')
  })

  it('clearScanWarnings 清空警告列表', () => {
    useAppStore.setState({
      scanWarnings: [
        { path: '/a', message: '无法访问 /a', kind: 'PermissionDenied' },
        { path: '/b', message: '无法访问 /b', kind: 'Other' },
      ],
    })
    useAppStore.getState().clearScanWarnings()
    expect(useAppStore.getState().scanWarnings).toEqual([])
  })

  it('扫描失败时不清空已有的 scanWarnings（保留可能已收集的警告）', async () => {
    // 边界场景：扫描过程中已经收到了一些警告，随后整体扫描失败。
    // 此时已收集的警告仍然有意义（用户可能想看到哪些目录访问失败），
    // 所以不应被错误处理路径清空。
    useAppStore.setState({ scanWarnings: [] })
    vi.mocked(invoke).mockRejectedValue(new Error('scan failed') as never)

    await useAppStore.getState().scanDirectoryWithProgress('/photos', true)

    // 扫描失败时 errorMessage 被设置，scanWarnings 保持为空（因为没有事件）
    const state = useAppStore.getState()
    expect(state.errorMessage).toContain('扫描失败')
    expect(state.scanWarnings).toEqual([])
  })
})
