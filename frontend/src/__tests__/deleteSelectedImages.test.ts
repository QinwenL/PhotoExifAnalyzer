import { beforeEach, describe, expect, it, vi } from 'vitest'

// Mock @tauri-apps/api so store.ts can be imported in a node test env
vi.mock('@tauri-apps/api/tauri', () => ({
  invoke: vi.fn(),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'
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
      deleteProcessed: null,
      deleteTotal: null,
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

/**
 * P2.3: 删除进度条规格实现
 *
 * Spec (file-management/spec.md "批量操作进度"):
 * - WHEN 用户删除超过 10 张图片
 * - THEN 系统 SHALL 显示进度条
 * - AND 进度条 SHALL 显示已完成数量/总数量
 *
 * 这些测试验证 store 在删除过程中跟踪 deleteProcessed / deleteTotal，
 * 让 StatusBar 能渲染 "N / M" 格式的进度。
 */
describe('P2.3: delete progress counts', () => {
  // 捕获 listen 注册的 delete_progress 回调，让测试能手动触发进度事件
  let deleteProgressCallback: ((payload: unknown) => void) | null = null

  beforeEach(() => {
    vi.clearAllMocks()
    deleteProgressCallback = null
    vi.mocked(listen).mockImplementation(((event: string, cb: (e: { payload: unknown }) => void) => {
      if (event === 'delete_progress') {
        deleteProgressCallback = (payload: unknown) => cb({ payload })
      }
      return Promise.resolve(() => {})
    }) as never)

    useAppStore.setState({
      scanResults: [],
      filteredResults: [],
      selectedImages: new Set(),
      selectedDetailImage: null,
      lastSelectedIndex: null,
      isDeleting: false,
      deleteProgress: 0,
      deleteProcessed: null,
      deleteTotal: null,
      lastDeleteFailures: null,
    })
  })

  it('删除开始时设置 deleteTotal 为选中数量，deleteProcessed 为 0', async () => {
    const paths = Array.from({ length: 15 }, (_, i) => `/img_${i}.jpg`)
    const results = paths.map(makeResult)
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(paths),
    })

    // 让 invoke pending 直到我们手动 resolve，使我们能观察到 "删除开始" 时的状态
    let resolveInvoke!: (v: unknown[]) => void
    vi.mocked(invoke).mockImplementation(
      () => new Promise((res) => { resolveInvoke = res as (v: unknown[]) => void })
    )

    const promise = useAppStore.getState().deleteSelectedImages()

    // 在 invoke resolve 之前检查状态 —— 此时 deleteSelectedImages 应已
    // 同步设置 deleteTotal / deleteProcessed = 0。
    const state = useAppStore.getState()
    expect(state.isDeleting).toBe(true)
    expect(state.deleteTotal).toBe(15)
    expect(state.deleteProcessed).toBe(0)

    // 清理：让 promise 落定
    resolveInvoke(paths.map(() => ({ Ok: null })))
    await promise
  })

  it('收到 delete_progress 事件时同时更新 deleteProcessed 和 deleteProgress', async () => {
    const paths = Array.from({ length: 20 }, (_, i) => `/img_${i}.jpg`)
    const results = paths.map(makeResult)
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(paths),
    })

    // 让 invoke pending 直到我们手动 resolve
    let resolveInvoke!: (v: unknown[]) => void
    vi.mocked(invoke).mockImplementation(
      () => new Promise((res) => { resolveInvoke = res as (v: unknown[]) => void })
    )

    const promise = useAppStore.getState().deleteSelectedImages()

    // 模拟后端发来进度：10/20 完成 = 50%
    deleteProgressCallback?.({ processed: 10, total: 20, percentage: 50 })
    await Promise.resolve() // flush microtasks

    let state = useAppStore.getState()
    expect(state.deleteProgress).toBe(50)
    expect(state.deleteProcessed).toBe(10)
    expect(state.deleteTotal).toBe(20)

    // 模拟后端发来 100% 进度
    deleteProgressCallback?.({ processed: 20, total: 20, percentage: 100 })
    await Promise.resolve()

    state = useAppStore.getState()
    expect(state.deleteProgress).toBe(100)
    expect(state.deleteProcessed).toBe(20)

    // 完成 invoke，清理
    resolveInvoke(paths.map(() => ({ Ok: null })))
    await promise
  })

  it('删除完成后复位 deleteProcessed 和 deleteTotal', async () => {
    const paths = Array.from({ length: 12 }, (_, i) => `/img_${i}.jpg`)
    const results = paths.map(makeResult)
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(paths),
    })

    vi.mocked(invoke).mockResolvedValue(paths.map(() => ({ Ok: null })) as never)

    await useAppStore.getState().deleteSelectedImages()

    const state = useAppStore.getState()
    expect(state.isDeleting).toBe(false)
    // 完成后 processed/total 应清空（null），不再显示进度
    expect(state.deleteProcessed).toBeNull()
    expect(state.deleteTotal).toBeNull()
  })

  it('删除失败时也复位 deleteProcessed 和 deleteTotal', async () => {
    const paths = Array.from({ length: 12 }, (_, i) => `/img_${i}.jpg`)
    const results = paths.map(makeResult)
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(paths),
    })

    vi.mocked(invoke).mockRejectedValue(new Error('IPC failure'))

    await useAppStore.getState().deleteSelectedImages()

    const state = useAppStore.getState()
    expect(state.isDeleting).toBe(false)
    expect(state.deleteProcessed).toBeNull()
    expect(state.deleteTotal).toBeNull()
    expect(state.errorMessage).toContain('删除失败')
  })

  it('兼容旧版后端只发百分比数字的 payload', async () => {
    // 旧版后端 emit 的是裸数字（百分比），没有 processed/total 字段。
    // store 必须仍能从百分比 + 已知 deleteTotal 推导出 deleteProcessed，
    // 保证升级时不丢失进度显示。
    const paths = Array.from({ length: 20 }, (_, i) => `/img_${i}.jpg`)
    const results = paths.map(makeResult)
    useAppStore.setState({
      scanResults: results,
      filteredResults: results,
      selectedImages: new Set(paths),
    })

    let resolveInvoke!: (v: unknown[]) => void
    vi.mocked(invoke).mockImplementation(
      () => new Promise((res) => { resolveInvoke = res as (v: unknown[]) => void })
    )

    const promise = useAppStore.getState().deleteSelectedImages()

    // 旧版 payload：裸数字 50（表示 50%）
    deleteProgressCallback?.(50)
    await Promise.resolve()

    const state = useAppStore.getState()
    expect(state.deleteProgress).toBe(50)
    // 50% of 20 = 10
    expect(state.deleteProcessed).toBe(10)
    expect(state.deleteTotal).toBe(20)

    resolveInvoke(paths.map(() => ({ Ok: null })))
    await promise
  })
})
