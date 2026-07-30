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

  // Actions
  setSelectedDirectory: (dir: string | null) => void
  scanDirectory: (dir: string, recursive: boolean) => Promise<void>
  updateStatistics: () => void
  setFilterCriteria: (criteria: FilterCriteria) => void
  applyFilter: () => void
  deleteSelectedImages: () => Promise<void>
  toggleImageSelection: (path: string) => void
  selectAllImages: () => void
  clearSelection: () => void
  setViewMode: (mode: 'grid' | 'list') => void
  setSelectedDetailImage: (result: ScanResult | null) => void
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

  toggleImageSelection: (path) => {
    const { selectedImages } = get()
    const newSelection = new Set(selectedImages)
    if (newSelection.has(path)) {
      newSelection.delete(path)
    } else {
      newSelection.add(path)
    }
    set({ selectedImages: newSelection })
  },

  selectAllImages: () => {
    const { filteredResults } = get()
    const allPaths = new Set(filteredResults.map((r) => r.path))
    set({ selectedImages: allPaths })
  },

  clearSelection: () => set({ selectedImages: new Set() }),

  setViewMode: (mode) => set({ viewMode: mode }),

  setSelectedDetailImage: (result) => set({ selectedDetailImage: result }),
}))
