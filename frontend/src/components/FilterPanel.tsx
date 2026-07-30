import { useState } from 'react'
import { useAppStore, type FilterCriteria } from '../store'
import { Button } from '@/components/ui/button'

export function FilterPanel() {
  const {
    cameraStats,
    lensStats,
    filterCriteria,
    setFilterCriteria,
    applyFilter,
  } = useAppStore()

  const [isExpanded, setIsExpanded] = useState(false)
  const [localCriteria, setLocalCriteria] = useState<FilterCriteria>({ ...filterCriteria })

  // Camera multi-select
  const [selectedCameras, setSelectedCameras] = useState<Set<string>>(
    new Set(filterCriteria.cameras || [])
  )

  // Lens multi-select
  const [selectedLenses, setSelectedLenses] = useState<Set<string>>(
    new Set(filterCriteria.lenses || [])
  )

  // Range filters
  const [focalMin, setFocalMin] = useState(filterCriteria.focal_length?.[0]?.toString() || '')
  const [focalMax, setFocalMax] = useState(filterCriteria.focal_length?.[1]?.toString() || '')
  const [apertureMin, setApertureMin] = useState(filterCriteria.aperture?.[0]?.toString() || '')
  const [apertureMax, setApertureMax] = useState(filterCriteria.aperture?.[1]?.toString() || '')
  const [isoMin, setIsoMin] = useState(filterCriteria.iso?.[0]?.toString() || '')
  const [isoMax, setIsoMax] = useState(filterCriteria.iso?.[1]?.toString() || '')

  const toggleCamera = (camera: string) => {
    const newSelected = new Set(selectedCameras)
    if (newSelected.has(camera)) {
      newSelected.delete(camera)
    } else {
      newSelected.add(camera)
    }
    setSelectedCameras(newSelected)
  }

  const toggleLens = (lens: string) => {
    const newSelected = new Set(selectedLenses)
    if (newSelected.has(lens)) {
      newSelected.delete(lens)
    } else {
      newSelected.add(lens)
    }
    setSelectedLenses(newSelected)
  }

  const applyFilters = () => {
    const criteria: FilterCriteria = {
      and_mode: localCriteria.and_mode,
    }

    if (selectedCameras.size > 0) {
      criteria.cameras = Array.from(selectedCameras)
    }

    if (selectedLenses.size > 0) {
      criteria.lenses = Array.from(selectedLenses)
    }

    if (focalMin || focalMax) {
      criteria.focal_length = [
        focalMin ? parseFloat(focalMin) : 0,
        focalMax ? parseFloat(focalMax) : 1000,
      ]
    }

    if (apertureMin || apertureMax) {
      criteria.aperture = [
        apertureMin ? parseFloat(apertureMin) : 0,
        apertureMax ? parseFloat(apertureMax) : 100,
      ]
    }

    if (isoMin || isoMax) {
      criteria.iso = [
        isoMin ? parseInt(isoMin) : 0,
        isoMax ? parseInt(isoMax) : 100000,
      ]
    }

    setFilterCriteria(criteria)
    applyFilter()
  }

  const resetFilters = () => {
    setSelectedCameras(new Set())
    setSelectedLenses(new Set())
    setFocalMin('')
    setFocalMax('')
    setApertureMin('')
    setApertureMax('')
    setIsoMin('')
    setIsoMax('')
    setLocalCriteria({ and_mode: false })
    setFilterCriteria({ and_mode: false })
    applyFilter()
  }

  const hasActiveFilters =
    selectedCameras.size > 0 ||
    selectedLenses.size > 0 ||
    focalMin ||
    focalMax ||
    apertureMin ||
    apertureMax ||
    isoMin ||
    isoMax

  return (
    <div className="border-b border-border">
      <button
        className="w-full px-4 py-2 flex items-center justify-between hover:bg-muted"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <span className="text-sm font-medium">高级筛选</span>
        <span className="text-xs text-muted-foreground">
          {isExpanded ? '▼' : '▶'}
          {hasActiveFilters && ' (已启用)'}
        </span>
      </button>

      {isExpanded && (
        <div className="px-4 pb-4 space-y-4">
          {/* AND/OR Toggle */}
          <div className="flex items-center gap-2">
            <label className="text-sm text-muted-foreground">筛选逻辑:</label>
            <button
              className={`px-2 py-1 text-xs rounded ${
                localCriteria.and_mode
                  ? 'bg-primary text-primary-foreground'
                  : 'bg-muted text-muted-foreground'
              }`}
              onClick={() =>
                setLocalCriteria({ ...localCriteria, and_mode: !localCriteria.and_mode })
              }
            >
              {localCriteria.and_mode ? 'AND (全部满足)' : 'OR (任一满足)'}
            </button>
          </div>

          {/* Camera Multi-Select */}
          {cameraStats && cameraStats.cameras.length > 0 && (
            <div>
              <h4 className="text-sm font-medium text-muted-foreground mb-2">相机</h4>
              <div className="max-h-32 overflow-y-auto space-y-1">
                {cameraStats.cameras.slice(0, 20).map((camera) => (
                  <label
                    key={camera.name}
                    className="flex items-center gap-2 text-sm cursor-pointer hover:bg-muted p-1 rounded"
                  >
                    <input
                      type="checkbox"
                      checked={selectedCameras.has(camera.name)}
                      onChange={() => toggleCamera(camera.name)}
                      className="w-3 h-3"
                    />
                    <span className="truncate flex-1">{camera.name}</span>
                    <span className="text-xs text-muted-foreground">{camera.count}</span>
                  </label>
                ))}
              </div>
            </div>
          )}

          {/* Lens Multi-Select */}
          {lensStats && lensStats.lenses.length > 0 && (
            <div>
              <h4 className="text-sm font-medium text-muted-foreground mb-2">镜头</h4>
              <div className="max-h-32 overflow-y-auto space-y-1">
                {lensStats.lenses.slice(0, 20).map((lens) => (
                  <label
                    key={lens.name}
                    className="flex items-center gap-2 text-sm cursor-pointer hover:bg-muted p-1 rounded"
                  >
                    <input
                      type="checkbox"
                      checked={selectedLenses.has(lens.name)}
                      onChange={() => toggleLens(lens.name)}
                      className="w-3 h-3"
                    />
                    <span className="truncate flex-1">{lens.name}</span>
                    <span className="text-xs text-muted-foreground">{lens.count}</span>
                  </label>
                ))}
              </div>
            </div>
          )}

          {/* Focal Length Range */}
          <div>
            <h4 className="text-sm font-medium text-muted-foreground mb-2">焦距范围 (mm)</h4>
            <div className="flex gap-2">
              <input
                type="number"
                placeholder="最小"
                value={focalMin}
                onChange={(e) => setFocalMin(e.target.value)}
                className="w-full px-2 py-1 text-sm border rounded bg-background"
              />
              <span className="text-muted-foreground">-</span>
              <input
                type="number"
                placeholder="最大"
                value={focalMax}
                onChange={(e) => setFocalMax(e.target.value)}
                className="w-full px-2 py-1 text-sm border rounded bg-background"
              />
            </div>
          </div>

          {/* Aperture Range */}
          <div>
            <h4 className="text-sm font-medium text-muted-foreground mb-2">光圈范围</h4>
            <div className="flex gap-2">
              <input
                type="number"
                placeholder="最小"
                value={apertureMin}
                onChange={(e) => setApertureMin(e.target.value)}
                step="0.1"
                className="w-full px-2 py-1 text-sm border rounded bg-background"
              />
              <span className="text-muted-foreground">-</span>
              <input
                type="number"
                placeholder="最大"
                value={apertureMax}
                onChange={(e) => setApertureMax(e.target.value)}
                step="0.1"
                className="w-full px-2 py-1 text-sm border rounded bg-background"
              />
            </div>
          </div>

          {/* ISO Range */}
          <div>
            <h4 className="text-sm font-medium text-muted-foreground mb-2">ISO 范围</h4>
            <div className="flex gap-2">
              <input
                type="number"
                placeholder="最小"
                value={isoMin}
                onChange={(e) => setIsoMin(e.target.value)}
                className="w-full px-2 py-1 text-sm border rounded bg-background"
              />
              <span className="text-muted-foreground">-</span>
              <input
                type="number"
                placeholder="最大"
                value={isoMax}
                onChange={(e) => setIsoMax(e.target.value)}
                className="w-full px-2 py-1 text-sm border rounded bg-background"
              />
            </div>
          </div>

          {/* Action Buttons */}
          <div className="flex gap-2 pt-2">
            <Button size="sm" onClick={applyFilters}>
              应用筛选
            </Button>
            <Button size="sm" variant="outline" onClick={resetFilters}>
              重置
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}
