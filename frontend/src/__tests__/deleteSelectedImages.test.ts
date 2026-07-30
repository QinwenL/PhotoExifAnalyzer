import { beforeEach, describe, expect, it, vi } from 'vitest'

// Mock @tauri-apps/api so store.ts can be imported in a node test env
vi.mock('@tauri-apps/api/tauri', () => ({
  invoke: vi.fn(),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

import { invoke } from '@tauri-apps/api/tauri'
import { useAppStore, type ScanResult } from '../store'

function makeResult(path: string): ScanResult {
  return {
    path,
    exif: {},
    file_size: 0,
    error: undefined,
  }
}

/**
 * 验证 deleteSelectedImages 的正确行为：
 * - 成功删除后 lastSelectedIndex 必须重置（避免下次 Shift+Click 用陈旧索引）
 * - 部分失败时只移除成功删除的图片（不能把失败的也从列表移除）
 * - 整体调用失败时 isDeleting 必须复位，状态保持不变
 */
describe('deleteSelectedImages', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // 重置 store 到初始状态
    useAppStore.setState({
      scanResults: [],
      filteredResults: [],
      selectedImages: new Set(),
      selectedDetailImage: null,
      lastSelectedIndex: null,
      isDeleting: false,
      deleteProgress: 0,
    })
  })

  it('成功删除后重置 lastSelectedIndex 为 null', async () => {
    const results = [makeResult('/a.jpg'), makeResult('/b.jpg'), makeResult('/c.jpg')]
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(['/a.jpg', '/b.jpg']),
      lastSelectedIndex: 1,
    })

    // 模拟后端返回全部成功（serde: Ok(()) -> {Ok: null}）
    vi.mocked(invoke).mockResolvedValue([{ Ok: null }, { Ok: null }] as never)

    await useAppStore.getState().deleteSelectedImages()

    const state = useAppStore.getState()
    expect(state.lastSelectedIndex).toBeNull()
    expect(state.selectedImages.size).toBe(0)
    expect(state.isDeleting).toBe(false)
  })

  it('部分失败时只移除成功删除的图片', async () => {
    const results = [
      makeResult('/a.jpg'),
      makeResult('/b.jpg'),
      makeResult('/c.jpg'),
    ]
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(['/a.jpg', '/b.jpg', '/c.jpg']),
      lastSelectedIndex: 0,
    })

    // 模拟后端返回：a 成功、b 失败、c 成功
    // serde 默认序列化：Ok(()) -> {Ok: null}, Err("msg") -> {Err: "msg"}
    vi.mocked(invoke).mockResolvedValue([
      { Ok: null },
      { Err: 'Permission denied' },
      { Ok: null },
    ] as never)

    await useAppStore.getState().deleteSelectedImages()

    const state = useAppStore.getState()
    // a 和 c 被移除，b 保留
    expect(state.scanResults.map((r) => r.path)).toEqual(['/b.jpg'])
    expect(state.filteredResults.map((r) => r.path)).toEqual(['/b.jpg'])
    expect(state.selectedImages.size).toBe(0)
    expect(state.lastSelectedIndex).toBeNull()
  })

  it('后端调用抛错时 isDeleting 复位且状态不变', async () => {
    const results = [makeResult('/a.jpg'), makeResult('/b.jpg')]
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(['/a.jpg']),
      lastSelectedIndex: 0,
    })

    vi.mocked(invoke).mockRejectedValue(new Error('IPC failure'))

    await useAppStore.getState().deleteSelectedImages()

    const state = useAppStore.getState()
    expect(state.isDeleting).toBe(false)
    // 状态保持不变：未删除任何图片
    expect(state.scanResults).toHaveLength(2)
    expect(state.selectedImages.size).toBe(1)
  })

  it('删除完成后调用 updateStatistics 刷新统计', async () => {
    const results = [makeResult('/a.jpg'), makeResult('/b.jpg')]
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(['/a.jpg']),
      lastSelectedIndex: 0,
      // 提供一个空的 stats 让 updateStatistics 走完整路径
    })

    vi.mocked(invoke).mockResolvedValue([{ Ok: null }] as never)

    await useAppStore.getState().deleteSelectedImages()

    // updateStatistics 内部会调用 invoke('get_all_stats', ...)
    const invokeCalls = vi.mocked(invoke).mock.calls.map((c) => c[0])
    expect(invokeCalls).toContain('get_all_stats')
  })
})
