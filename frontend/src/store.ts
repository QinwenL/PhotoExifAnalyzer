import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'
import { create } from 'zustand'
import { formatCamera } from '@/lib/utils'

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

// App state
interface AppState {
  // Directory
  selectedDirectory: string | null
  isScanning: boolean
  scanProgress: number

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
    set({ isScanning: true, scanProgress: 0 })
    // Same rationale as scanDirectory (see comment above): rotate the
    // thumbnail epoch so stale-skeleton tiles re-fire their load effect
    // and in-flight completes from the cancelled previous batch are
    // gated out by the epoch check in completeThumbnailLoad.
    get().resetThumbnailProgress()
    try {
      const progressHandler = listen('scan_progress', (event) => {
        set({ scanProgress: event.payload as number })
      })

      const results: ScanResult[] = await invoke('scan_images_with_progress', { dir, recursive })
      await progressHandler
      set({
        scanResults: results,
        filteredResults: results,
        isScanning: false,
        scanProgress: 100,
      })
      get().updateStatistics()
    } catch (error) {
      console.error('Scan failed:', error)
      set({ isScanning: false })
    }
  },

  cancelScan: async () => {
    try {
      await invoke('cancel_scan')
      set({ isScanning: false, scanProgress: 0 })
    } catch (error) {
      console.error('Cancel scan failed:', error)
    }
  },

  updateStatistics: () => {
    const { scanResults } = get()
    if (scanResults.length === 0) return

    Promise.all([
      invoke<CameraStats>('get_camera_stats', { results: scanResults }),
      invoke<LensStats>('get_lens_stats', { results: scanResults }),
      invoke<FocalLengthStats>('get_focal_length_stats', { results: scanResults }),
    ]).then(([cameraStats, lensStats, focalLengthStats]) => {
      set({ cameraStats, lensStats, focalLengthStats })
    })
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
    await invoke('delete_images_with_progress', { paths })

    await progressHandler

    // Remove deleted images from results
    const remaining = scanResults.filter((r) => !selectedImages.has(r.path))
    set({
      scanResults: remaining,
      filteredResults: remaining,
      selectedImages: new Set(),
      isDeleting: false,
      deleteProgress: 100,
    })

    // Update statistics
    get().updateStatistics()
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

  clearSelection: () => set({ selectedImages: new Set() }),

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

// Helper function to sort results
function sortResults(
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
