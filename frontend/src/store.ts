import { invoke } from '@tauri-apps/api/tauri'
import { create } from 'zustand'

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
  and_mode: boolean
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

  // Actions
  setSelectedDirectory: (dir: string | null) => void
  scanDirectory: (dir: string, recursive: boolean) => Promise<void>
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
  exportToJSON: () => void
}

export const useAppStore = create<AppState>((set, get) => ({
  // Initial state
  selectedDirectory: null,
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

  // Actions
  setSelectedDirectory: (dir) => set({ selectedDirectory: dir }),

  scanDirectory: async (dir, recursive) => {
    set({ isScanning: true, scanProgress: 0 })
    try {
      const results: ScanResult[] = await invoke('scan_images', { dir, recursive })
      set({
        scanResults: results,
        filteredResults: results,
        isScanning: false,
        scanProgress: 100,
      })
      // Update statistics
      get().updateStatistics()
    } catch (error) {
      console.error('Scan failed:', error)
      set({ isScanning: false })
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

    const paths = Array.from(selectedImages)
    await invoke('delete_images', { paths })

    // Remove deleted images from results
    const remaining = scanResults.filter((r) => !selectedImages.has(r.path))
    set({
      scanResults: remaining,
      filteredResults: remaining,
      selectedImages: new Set(),
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

  exportToJSON: () => {
    const { filteredResults, cameraStats, lensStats, focalLengthStats } = get()
    const exportData = {
      timestamp: new Date().toISOString(),
      totalImages: filteredResults.length,
      statistics: {
        cameras: cameraStats,
        lenses: lensStats,
        focalLength: focalLengthStats,
      },
      images: filteredResults.map((r) => ({
        path: r.path,
        filename: r.path.split(/[/\\]/).pop(),
        size: r.file_size,
        camera: r.exif.make && r.exif.model ? `${r.exif.make} ${r.exif.model}` : null,
        lens: r.exif.lens_model || null,
        focalLength: r.exif.focal_length || null,
        aperture: r.exif.aperture || null,
        shutterSpeed: r.exif.exposure_time || null,
        iso: r.exif.iso || null,
        datetime: r.exif.datetime_original || null,
      })),
    }

    const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `exif-export-${new Date().toISOString().slice(0, 10)}.json`
    a.click()
    URL.revokeObjectURL(url)
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
      case 'camera':
        const cameraA = a.exif.make && a.exif.model ? `${a.exif.make} ${a.exif.model}` : ''
        const cameraB = b.exif.make && b.exif.model ? `${b.exif.make} ${b.exif.model}` : ''
        comparison = cameraA.localeCompare(cameraB)
        break
    }

    return order === 'asc' ? comparison : -comparison
  })

  return sorted
}
