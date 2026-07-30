import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'
import { formatCamera } from '@/lib/utils'
import { interpretDeleteResults } from '@/lib/delete-result'

// Types matching Rust backend
export interface ExifData {
  make?: string
  model?: string
  lens_model?: string
  focal_length?: number
  aperture?: number
  iso?: number
  exposure_time?: number
  exposure_program?: string
  metering_mode?: string
  flash?: boolean
  white_balance?: string
  image_width?: number
  image_height?: number
  datetime_original?: string
  gps_latitude?: number
  gps_longitude?: number
}

export interface ScanResult {
  path: string
  exif: ExifData
  file_size: number
  error?: string
}

export interface StatItem {
  name: string
  count: number
  percentage: number
}

export interface FocalRange {
  label: string
  min: number
  max: number
  count: number
  percentage: number
}

export interface CameraStats {
  cameras: StatItem[]
  total: number
}

export interface LensStats {
  lenses: StatItem[]
  total: number
}

export interface FocalLengthStats {
  ranges: FocalRange[]
  total: number
}

export interface FilterCriteria {
  cameras?: string[]
  lenses?: string[]
  focal_length?: [number, number]
  aperture?: [number, number]
  iso?: [number, number]
  exposure_time?: [number, number]
  date_range?: [string, string]
  and_mode: boolean
}

// Export payload returned by the backend `export_statistics` command.
// Mirrors `ExportData` / `ExportImage` in `src-tauri/src/lib.rs`.
export interface ExportImage {
  path: string
  filename: string
  size: number
  camera: string | null
  lens: string | null
  focal_length: number | null
  aperture: number | null
  shutter_speed: number | null
  iso: number | null
  datetime: string | null
}

export interface AllStats {
  cameras: CameraStats
  lenses: LensStats
  focal_length: FocalLengthStats
}

export interface ExportStatistics {
  cameras: CameraStats
  lenses: LensStats
  focal_length: FocalLengthStats
}

export interface ExportData {
  timestamp: string
  total_images: number
  statistics: ExportStatistics
  images: ExportImage[]
}

// Scan progress payload emitted by the Rust backend.
// Mirrors `ScanProgressPayload` in `src-tauri/src/exif/scanner.rs`.
// Carries processed/total so the UI can display "scanned N / M".
export interface ScanProgressPayload {
  processed: number
  total: number
  percentage: number
}

// App state
interface AppState {
  // Directory
  selectedDirectory: string | null
  isScanning: boolean
  scanProgress: number
  // P2.2: 已扫描 / 总数。null 表示当前未在扫描或后端尚未 emit 任何 payload。
  scanProcessed: number | null
  scanTotal: number | null

  // 用户可见的错误消息（null 表示无错误）。捕获原本只 console.error
  // 的失败（扫描失败、取消失败、图片加载失败等），让 UI 能向用户反馈。
  errorMessage: string | null
  clearErrorMessage: () => void

  // P1.7: EXIF 缓存初始化失败时的警告消息（null 表示缓存正常或尚未查询）。
  // 由 App 挂载时调用 get_cache_status 获取；非 null 时 UI 显示非阻塞警告，
  // 告知用户重复扫描会变慢。
  cacheWarning: string | null
  clearCacheWarning: () => void
  queryCacheStatus: () => Promise<void>

  // Data
  scanResults: ScanResult[]
  filteredResults: ScanResult[]

  // Statistics
  cameraStats: CameraStats | null
  lensStats: LensStats | null
  focalLengthStats: FocalLengthStats | null

  // Filter
  filterCriteria: FilterCriteria

  // UI state
  viewMode: 'grid' | 'list'
  selectedImages: Set<string>
  selectedDetailImage: ScanResult | null
  lastSelectedIndex: number | null
  sortBy: 'name' | 'date' | 'size' | 'camera'
  sortOrder: 'asc' | 'desc'
  theme: 'light' | 'dark' | 'system'
  detailMode: 'simple' | 'detailed'

  // Delete progress
  isDeleting: boolean
  deleteProgress: number
  /** 最近一次批量删除中失败的路径与错误消息（null 表示无失败或未删除） */
  lastDeleteFailures: Array<{ path: string; error: string }> | null

  // Thumbnail loading progress
  // `thumbnailEpoch` is incremented whenever a new scan starts so that
  // settled promises from an earlier-scan batch are no-op'ed against
  // the fresh counters (prevents cross-scan counter drift).
  thumbnailEpoch: number
  thumbnailPending: number
  thumbnailLoaded: number
  thumbnailErrors: number

  // Actions
  setSelectedDirectory: (dir: string | null) => void
  scanDirectoryWithProgress: (dir: string, recursive: boolean) => Promise<void>
  cancelScan: () => Promise<void>
  updateStatistics: () => void
  setFilterCriteria: (criteria: FilterCriteria) => void
  applyFilter: () => void
  deleteSelectedImages: () => Promise<void>
  toggleImageSelection: (path: string, index: number, shiftKey: boolean, ctrlKey: boolean) => void
  selectAllImages: () => void
  clearSelection: () => void
  /** 清空当前选择并仅选中给定路径（用于单图删除等场景） */
  selectSingleImage: (path: string) => void
  setViewMode: (mode: 'grid' | 'list') => void
  setSelectedDetailImage: (result: ScanResult | null) => void
  setSortBy: (field: 'name' | 'date' | 'size' | 'camera') => void
  toggleSortOrder: () => void
  setTheme: (theme: 'light' | 'dark' | 'system') => void
  setDetailMode: (mode: 'simple' | 'detailed') => void
  exportToJSON: () => Promise<void>
  filterByCamera: (camera: string) => void
  filterByLens: (lens: string) => void
  resetFilter: () => void
  beginThumbnailLoad: (path: string, epoch: number) => void
  completeThumbnailLoad: (path: string, ok: boolean, epoch: number) => void
  resetThumbnailProgress: () => void
}

export const useAppStore = create<AppState>((set, get) => ({
  // Initial state
  selectedDirectory: (() => {
    try { return localStorage.getItem('lastDirectory') } catch { return null }
  })(),
  isScanning: false,
  scanProgress: 0,
  scanProcessed: null,
  scanTotal: null,
  errorMessage: null,
  clearErrorMessage: () => set({ errorMessage: null }),
  cacheWarning: null,
  clearCacheWarning: () => set({ cacheWarning: null }),
  queryCacheStatus: async () => {
    try {
      const [available, error] = await invoke<[boolean, string | null]>('get_cache_status')
      if (!available && error) {
        set({ cacheWarning: `EXIF 缓存不可用：${error}（重复扫描会变慢）` })
      } else {
        set({ cacheWarning: null })
      }
    } catch {
      // 命令不存在或调用失败时不阻塞应用启动
    }
  },
  scanResults: [],
  filteredResults: [],
  cameraStats: null,
  lensStats: null,
  focalLengthStats: null,
  filterCriteria: { and_mode: false },
  viewMode: 'grid',
  selectedImages: new Set(),
  selectedDetailImage: null,
  lastSelectedIndex: null,
  sortBy: 'name',
  sortOrder: 'asc',
  theme: (() => {
    try { return (localStorage.getItem('theme') as 'light' | 'dark' | 'system') || 'system' } catch { return 'system' }
  })(),
  detailMode: 'simple',
  isDeleting: false,
  deleteProgress: 0,
  lastDeleteFailures: null,
  thumbnailEpoch: 0,
  thumbnailPending: 0,
  thumbnailLoaded: 0,
  thumbnailErrors: 0,

  // Actions
  setSelectedDirectory: (dir) => {
    set({ selectedDirectory: dir })
    if (dir) {
      try { localStorage.setItem('lastDirectory', dir) } catch { /* ignore */ }
    }
  },

  scanDirectoryWithProgress: async (dir, recursive) => {
    set({ isScanning: true, scanProgress: 0, scanProcessed: null, scanTotal: null })
    // Same rationale as scanDirectory (see comment above): rotate the
    // thumbnail epoch so stale-skeleton tiles re-fire their load effect
    // and in-flight completes from the cancelled previous batch are
    // gated out by the epoch check in completeThumbnailLoad.
    get().resetThumbnailProgress()
    try {
      const progressHandler = listen('scan_progress', (event) => {
        const payload = event.payload as ScanProgressPayload
        set({
          scanProgress: payload.percentage,
          scanProcessed: payload.processed,
          scanTotal: payload.total,
        })
      })

      const results: ScanResult[] = await invoke('scan_images_with_progress', { dir, recursive })
      await progressHandler
      set({
        scanResults: results,
        filteredResults: results,
        isScanning: false,
        scanProgress: 100,
        scanProcessed: results.length,
        scanTotal: results.length,
      })
      get().updateStatistics()
    } catch (error) {
      console.error('Scan failed:', error)
      set({
        isScanning: false,
        errorMessage: `扫描失败：${error instanceof Error ? error.message : String(error)}`,
      })
    }
  },

  cancelScan: async () => {
    try {
      await invoke('cancel_scan')
      set({ isScanning: false, scanProgress: 0, scanProcessed: null, scanTotal: null })
    } catch (error) {
      console.error('Cancel scan failed:', error)
      set({
        errorMessage: `取消扫描失败：${error instanceof Error ? error.message : String(error)}`,
      })
    }
  },

  updateStatistics: () => {
    const { scanResults } = get()
    if (scanResults.length === 0) return

    // Single invoke across the Tauri IPC boundary: the full scanResults
    // array is serialized and transmitted exactly ONCE, not three times as
    // with the previous `Promise.all([get_camera_stats, get_lens_stats,
    // get_focal_length_stats])` pattern. For large photo libraries this is
    // the biggest win in the "scan completes to 100% then hangs forever
    // while stats load" symptom.
    invoke<AllStats>('get_all_stats', { results: scanResults }).then(
      (all) => {
        set({
          cameraStats: all.cameras,
          lensStats: all.lenses,
          focalLengthStats: all.focal_length,
        })
      }
    )
  },

  setFilterCriteria: (criteria) => set({ filterCriteria: criteria }),

  applyFilter: () => {
    const { scanResults, filterCriteria } = get()
    invoke<ScanResult[]>('filter_images', { results: scanResults, criteria: filterCriteria }).then(
      (filtered: ScanResult[]) => {
        set({ filteredResults: filtered })
      }
    )
  },

  deleteSelectedImages: async () => {
    const { selectedImages, scanResults } = get()
    if (selectedImages.size === 0) return

    set({ isDeleting: true, deleteProgress: 0 })

    const progressHandler = listen('delete_progress', (event) => {
      set({ deleteProgress: event.payload as number })
    })

    const paths = Array.from(selectedImages)

    try {
      // 后端返回 Vec<Result<(), String>>：每个元素对应一个路径的删除结果
      const results = await invoke<unknown[]>('delete_images_with_progress', { paths })
      await progressHandler

      const outcome = interpretDeleteResults(results, paths)
      const deletedPaths = new Set(
        outcome.succeededIndices.map((i) => paths[i])
      )

      // 仅移除成功删除的图片，失败的保留在列表中
      const remaining = scanResults.filter((r) => !deletedPaths.has(r.path))

      set({
        scanResults: remaining,
        filteredResults: remaining,
        selectedImages: new Set(),
        // 重置 lastSelectedIndex 避免下次 Shift+Click 使用陈旧索引
        lastSelectedIndex: null,
        isDeleting: false,
        deleteProgress: 100,
        // 暴露失败信息供 UI 提示（如有）
        lastDeleteFailures: outcome.failedIndices.size > 0
          ? Array.from(outcome.failedIndices.entries()).map(([i, err]) => ({
              path: paths[i],
              error: err,
            }))
          : null,
      })

      // Update statistics
      get().updateStatistics()
    } catch (error) {
      // 整体调用失败（IPC 错误等）：复位 isDeleting，保留状态不变
      await progressHandler.catch(() => {})
      console.error('Delete failed:', error)
      set({
        isDeleting: false,
        deleteProgress: 0,
        errorMessage: `删除失败：${error instanceof Error ? error.message : String(error)}`,
      })
    }
  },

  toggleImageSelection: (path, index, shiftKey, ctrlKey) => {
    const { selectedImages, filteredResults, lastSelectedIndex } = get()
    const newSelection = new Set(selectedImages)

    if (shiftKey && lastSelectedIndex !== null) {
      // Shift+click: select range from lastSelected to current
      const start = Math.min(lastSelectedIndex, index)
      const end = Math.max(lastSelectedIndex, index)
      for (let i = start; i <= end; i++) {
        if (i < filteredResults.length) {
          newSelection.add(filteredResults[i].path)
        }
      }
    } else if (ctrlKey) {
      // Ctrl+click: toggle single item
      if (newSelection.has(path)) {
        newSelection.delete(path)
      } else {
        newSelection.add(path)
      }
    } else {
      // Normal click: clear selection and select only this item
      newSelection.clear()
      newSelection.add(path)
    }

    set({
      selectedImages: newSelection,
      lastSelectedIndex: index,
    })
  },

  selectAllImages: () => {
    const { filteredResults } = get()
    const allPaths = new Set(filteredResults.map((r) => r.path))
    set({ selectedImages: allPaths })
  },

  clearSelection: () => set({ selectedImages: new Set(), lastSelectedIndex: null }),

  selectSingleImage: (path) => set({
    selectedImages: new Set([path]),
    // 重置 lastSelectedIndex：单图选择不建立范围选择的起点
    lastSelectedIndex: null,
  }),

  setViewMode: (mode) => set({ viewMode: mode }),

  setSelectedDetailImage: (result) => set({ selectedDetailImage: result }),

  setTheme: (theme) => {
    set({ theme })
    try { localStorage.setItem('theme', theme) } catch { /* ignore */ }
    // Apply theme to document
    const root = document.documentElement
    root.classList.remove('light', 'dark')
    if (theme === 'system') {
      const systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches
      root.classList.add(systemDark ? 'dark' : 'light')
    } else {
      root.classList.add(theme)
    }
  },

  setSortBy: (field) => {
    const { filteredResults, sortOrder } = get()
    const sorted = sortResults(filteredResults, field, sortOrder)
    set({ sortBy: field, filteredResults: sorted })
  },

  toggleSortOrder: () => {
    const { filteredResults, sortBy, sortOrder } = get()
    const newOrder = sortOrder === 'asc' ? 'desc' : 'asc'
    const sorted = sortResults(filteredResults, sortBy, newOrder)
    set({ sortOrder: newOrder, filteredResults: sorted })
  },

  exportToJSON: async () => {
    const { filteredResults } = get()
    // Delegate field mapping + statistics aggregation to the backend
    // `export_statistics` command so the logic lives in one place
    // (mirrors `ExportData` in `src-tauri/src/lib.rs`).
    const exportData = await invoke<ExportData>('export_statistics', {
      results: filteredResults,
    })

    const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `exif-export-${new Date().toISOString().slice(0, 10)}.json`
    a.click()
    URL.revokeObjectURL(url)
  },

  setDetailMode: (mode) => set({ detailMode: mode }),

  filterByCamera: (camera) => {
    const { scanResults } = get()
    const criteria: FilterCriteria = { cameras: [camera], and_mode: false }
    set({ filterCriteria: criteria })
    invoke<ScanResult[]>('filter_images', { results: scanResults, criteria }).then(
      (filtered) => set({ filteredResults: filtered })
    )
  },

  filterByLens: (lens) => {
    const { scanResults } = get()
    const criteria: FilterCriteria = { lenses: [lens], and_mode: false }
    set({ filterCriteria: criteria })
    invoke<ScanResult[]>('filter_images', { results: scanResults, criteria }).then(
      (filtered) => set({ filteredResults: filtered })
    )
  },

  resetFilter: () => {
    const { scanResults } = get()
    set({ filterCriteria: { and_mode: false }, filteredResults: scanResults })
  },

  beginThumbnailLoad: (path, epoch) => {
    void path
    // Functional set guarantees atomic read→update even with concurrent
    // microtasks from many Thumbnail components (eliminates lost updates).
    // Drop calls from previous scan epochs entirely.
    set((state) => (epoch !== state.thumbnailEpoch ? {} : {
      thumbnailPending: state.thumbnailPending + 1,
    }))
  },

  completeThumbnailLoad: (path, ok, epoch) => {
    void path
    set((state) => {
      if (epoch !== state.thumbnailEpoch) return {}
      const pending = Math.max(0, state.thumbnailPending - 1)
      if (ok) {
        return {
          thumbnailPending: pending,
          thumbnailLoaded: state.thumbnailLoaded + 1,
        }
      }
      return {
        thumbnailPending: pending,
        thumbnailErrors: state.thumbnailErrors + 1,
      }
    })
  },

  resetThumbnailProgress: () => {
    set((state) => ({
      thumbnailEpoch: state.thumbnailEpoch + 1,
      thumbnailPending: 0,
      thumbnailLoaded: 0,
      thumbnailErrors: 0,
    }))
  },
}))

// Helper function to sort results.
// Exported for unit testing — not part of the public store API.
export function sortResults(
  results: ScanResult[],
  sortBy: 'name' | 'date' | 'size' | 'camera',
  order: 'asc' | 'desc'
): ScanResult[] {
  const sorted = [...results].sort((a, b) => {
    let comparison = 0

    switch (sortBy) {
      case 'name':
        comparison = a.path.localeCompare(b.path)
        break
      case 'date':
        comparison = (a.exif.datetime_original || '').localeCompare(b.exif.datetime_original || '')
        break
      case 'size':
        comparison = a.file_size - b.file_size
        break
      case 'camera': {
        const cameraA = formatCamera(a.exif) || ''
        const cameraB = formatCamera(b.exif) || ''
        comparison = cameraA.localeCompare(cameraB)
        break
      }
    }

    return order === 'asc' ? comparison : -comparison
  })

  return sorted
}
