import React, { useEffect, useState } from 'react'
import { open } from '@tauri-apps/api/dialog'
import { useAppStore, type ScanResult } from './store'
import { Button } from '@/components/ui/button'
import { FocalLengthChart } from './components/FocalLengthChart'
import { DistributionChart } from './components/DistributionChart'
import { FilterPanel } from './components/FilterPanel'
import { ImageDetail } from './components/ImageDetail'
import { VirtualizedGrid, VirtualizedList } from './components/VirtualizedGrid'
import { StatusBar } from './components/StatusBar'
import { ConfirmDialog } from './components/ConfirmDialog'
import { Thumbnail } from './components/Thumbnail'
import { formatCamera } from '@/lib/utils'
import { getKeyboardAction, isTextInputTarget } from '@/lib/keyboard'

// Responsive grid columns based on container width
function useGridColumns() {
  const [columns, setColumns] = React.useState(5)

  React.useEffect(() => {
    const updateColumns = () => {
      const width = window.innerWidth - 320 // subtract sidebar
      if (width < 640) setColumns(2)
      else if (width < 768) setColumns(3)
      else if (width < 1024) setColumns(4)
      else setColumns(5)
    }
    updateColumns()
    window.addEventListener('resize', updateColumns)
    return () => window.removeEventListener('resize', updateColumns)
  }, [])

  return columns
}

function App() {
  const columns = useGridColumns()
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false)
  const [pendingDeleteImages, setPendingDeleteImages] = useState<ScanResult[]>([])
  const {
    isScanning,
    scanProgress,
    filteredResults,
    cameraStats,
    lensStats,
    focalLengthStats,
    viewMode,
    selectedImages,
    selectedDetailImage,
    sortBy,
    sortOrder,
    theme,
    errorMessage,
    clearErrorMessage,
    setSelectedDirectory,
    scanDirectoryWithProgress,
    cancelScan,
    deleteSelectedImages,
    setViewMode,
    selectAllImages,
    clearSelection,
    selectSingleImage,
    setSelectedDetailImage,
    setSortBy,
    toggleSortOrder,
    setTheme,
    exportToJSON,
    filterByCamera,
    filterByLens,
    detailMode,
    setDetailMode,
  } = useAppStore()

  // Global keyboard shortcuts (Ctrl+A, Delete, Escape)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const action = getKeyboardAction(e, {
        hasSelection: selectedImages.size > 0,
        hasDetailOpen: !!selectedDetailImage,
        hasResults: filteredResults.length > 0,
        isTextInput: isTextInputTarget(e.target),
      })

      switch (action.type) {
        case 'selectAll': {
          // 阻止浏览器默认文本选择行为，改为选中所有图片
          e.preventDefault()
          selectAllImages()
          break
        }
        case 'delete': {
          const imagesToDelete = filteredResults.filter((r) => selectedImages.has(r.path))
          setPendingDeleteImages(imagesToDelete)
          setShowDeleteConfirm(true)
          break
        }
        case 'escape': {
          setSelectedDetailImage(null)
          break
        }
        case 'none':
          break
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [selectedImages, selectedDetailImage, filteredResults, selectAllImages, setSelectedDetailImage])

  // Apply theme on mount
  useEffect(() => {
    setTheme(theme)
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const handleSelectDirectory = async () => {
    const selected = await open({ directory: true, multiple: false })
    if (selected && typeof selected === 'string') {
      setSelectedDirectory(selected)
      await scanDirectoryWithProgress(selected, true)
    }
  }

  const handleDelete = async () => {
    if (selectedImages.size === 0) return
    const imagesToDelete = filteredResults.filter((r) => selectedImages.has(r.path))
    setPendingDeleteImages(imagesToDelete)
    setShowDeleteConfirm(true)
  }

  const confirmDelete = async () => {
    await deleteSelectedImages()
    setShowDeleteConfirm(false)
    setPendingDeleteImages([])
  }

  const cancelDelete = () => {
    setShowDeleteConfirm(false)
    setPendingDeleteImages([])
  }

  /**
   * 单图删除入口（来自 ImageDetail 的"删除"按钮）。
   * 走与批量删除完全相同的 ConfirmDialog + deleteSelectedImages 路径，
   * 保证 scanResults / 统计 / lastSelectedIndex 同步更新。
   */
  const handleDeleteFromDetail = (result: ScanResult) => {
    setSelectedDetailImage(null)
    selectSingleImage(result.path)
    setPendingDeleteImages([result])
    setShowDeleteConfirm(true)
  }

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Header */}
      <header className="border-b border-border p-4">
        <div className="flex items-center justify-between">
          <h1 className="text-2xl font-bold">PhotoExifAnalyzer</h1>
          <div className="flex items-center gap-2">
            {/* Theme toggle */}
            <div className="flex gap-1 border border-border rounded p-1">
              <Button
                variant={theme === 'light' ? 'default' : 'ghost'}
                size="sm"
                className="text-xs"
                onClick={() => setTheme('light')}
              >
                ☀️
              </Button>
              <Button
                variant={theme === 'dark' ? 'default' : 'ghost'}
                size="sm"
                className="text-xs"
                onClick={() => setTheme('dark')}
              >
                🌙
              </Button>
              <Button
                variant={theme === 'system' ? 'default' : 'ghost'}
                size="sm"
                className="text-xs"
                onClick={() => setTheme('system')}
              >
                💻
              </Button>
            </div>
            <Button onClick={handleSelectDirectory} disabled={isScanning}>
              {isScanning ? '扫描中...' : '选择文件夹'}
            </Button>
            {selectedImages.size > 0 && (
              <Button variant="destructive" onClick={handleDelete}>
                删除选中 ({selectedImages.size})
              </Button>
            )}
          </div>
        </div>
      </header>

      {/* Error Banner */}
      {errorMessage && (
        <div className="bg-destructive/10 border-b border-destructive/30 px-4 py-2 flex items-center justify-between gap-4">
          <p className="text-sm text-destructive">{errorMessage}</p>
          <Button
            variant="ghost"
            size="sm"
            className="text-destructive hover:bg-destructive/20"
            onClick={clearErrorMessage}
          >
            ✕
          </Button>
        </div>
      )}

      {/* Main Content */}
      <div className="flex h-[calc(100vh-65px)]">
        {/* Sidebar - Statistics */}
        <aside className="w-80 border-r border-border overflow-y-auto">
          <div className="p-4">
            <h2 className="text-lg font-semibold mb-4">统计信息</h2>

            {cameraStats && cameraStats.cameras.length > 0 && (
              <div className="mb-6">
                <h3 className="text-sm font-medium text-muted-foreground mb-2">
                  相机 (共 {cameraStats.total} 张)
                </h3>
                <DistributionChart
                  data={cameraStats.cameras.map((c) => ({
                    name: c.name,
                    value: c.count,
                    percentage: c.percentage,
                  }))}
                  title="相机"
                  onItemClick={(name) => filterByCamera(name)}
                />
              </div>
            )}

            {lensStats && lensStats.lenses.length > 0 && (
              <div className="mb-6">
                <h3 className="text-sm font-medium text-muted-foreground mb-2">
                  镜头 (共 {lensStats.total} 张)
                </h3>
                <DistributionChart
                  data={lensStats.lenses.map((l) => ({
                    name: l.name,
                    value: l.count,
                    percentage: l.percentage,
                  }))}
                  title="镜头"
                  onItemClick={(name) => filterByLens(name)}
                />
              </div>
            )}

            {focalLengthStats && focalLengthStats.ranges.length > 0 && (
              <div className="mb-6">
                <h3 className="text-sm font-medium text-muted-foreground mb-2">
                  焦距分布 (共 {focalLengthStats.total} 张)
                </h3>
                <FocalLengthChart stats={focalLengthStats} />
              </div>
            )}

            {!cameraStats && !isScanning && (
              <p className="text-sm text-muted-foreground">选择一个文件夹开始分析</p>
            )}
          </div>

          {/* Filter Panel */}
          <FilterPanel />
        </aside>

        {/* Main Area */}
        <main className="flex-1 overflow-hidden p-4">
          {/* Toolbar */}
          <div className="flex items-center justify-between mb-4">
            <span className="text-sm text-muted-foreground">{filteredResults.length} 张图片</span>
            <div className="flex gap-2">
              {/* Sort buttons */}
              <div className="flex gap-1 border rounded">
                {(['name', 'date', 'size', 'camera'] as const).map((field) => (
                  <Button
                    key={field}
                    variant={sortBy === field ? 'default' : 'ghost'}
                    size="sm"
                    className="text-xs"
                    onClick={() => setSortBy(field)}
                  >
                    {field === 'name' && '名称'}
                    {field === 'date' && '日期'}
                    {field === 'size' && '大小'}
                    {field === 'camera' && '相机'}
                    {sortBy === field && (sortOrder === 'asc' ? ' ↑' : ' ↓')}
                  </Button>
                ))}
              </div>
              <Button variant="outline" size="sm" onClick={toggleSortOrder}>
                {sortOrder === 'asc' ? '升序' : '降序'}
              </Button>
              
              {/* View mode buttons */}
              <Button
                variant={viewMode === 'grid' ? 'default' : 'outline'}
                size="sm"
                onClick={() => setViewMode('grid')}
              >
                网格
              </Button>
              <Button
                variant={viewMode === 'list' ? 'default' : 'outline'}
                size="sm"
                onClick={() => setViewMode('list')}
              >
                列表
              </Button>
              {viewMode === 'list' && (
                <Button
                  variant={detailMode === 'detailed' ? 'default' : 'outline'}
                  size="sm"
                  onClick={() => setDetailMode(detailMode === 'simple' ? 'detailed' : 'simple')}
                >
                  {detailMode === 'simple' ? '详细' : '简洁'}
                </Button>
              )}
              {filteredResults.length > 0 && (
                <>
                  <Button variant="outline" size="sm" onClick={selectAllImages}>
                    全选
                  </Button>
                  <Button variant="outline" size="sm" onClick={clearSelection}>
                    取消选择
                  </Button>
                  <Button variant="outline" size="sm" onClick={exportToJSON}>
                    导出 JSON
                  </Button>
                </>
              )}
            </div>
          </div>

          {/* Image Grid/List */}
          <div className="h-[calc(100vh-180px)]">
            {viewMode === 'grid' ? (
              <VirtualizedGrid
                items={filteredResults}
                columns={columns}
                renderItem={(result, index) => (
                  <div
                    key={result.path}
                    className={`relative aspect-square border rounded-lg overflow-hidden cursor-pointer transition-all ${
                      selectedImages.has(result.path)
                        ? 'ring-2 ring-primary ring-offset-2'
                        : 'hover:ring-2 hover:ring-primary/50'
                    }`}
                    onClick={(e) => useAppStore.getState().toggleImageSelection(result.path, index, e.shiftKey, e.ctrlKey || e.metaKey)}
                    onDoubleClick={() => setSelectedDetailImage(result)}
                  >
                    <Thumbnail path={result.path} className="absolute inset-0" />
                    {/* Error indicator */}
                    {result.error && (
                      <div className="absolute top-2 left-2 bg-destructive/90 text-destructive-foreground text-[9px] px-1.5 py-0.5 rounded">
                        EXIF 解析失败
                      </div>
                    )}
                    {/* EXIF info overlay */}
                    <div className="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/90 via-black/70 to-transparent px-1.5 pt-4 pb-1">
                      <p className="text-[10px] text-white truncate font-medium">
                        {result.path.split(/[/\\]/).pop()}
                      </p>
                      <div className="flex flex-wrap gap-x-1.5 gap-y-0 mt-0.5">
                        {formatCamera(result.exif) && (
                          <span className="text-[9px] text-white/90 truncate max-w-full">
                            {formatCamera(result.exif)}
                          </span>
                        )}
                        {result.exif.lens_model && (
                          <span className="text-[9px] text-white/80 truncate max-w-full">
                            {result.exif.lens_model}
                          </span>
                        )}
                      </div>
                      <div className="flex flex-wrap gap-x-1.5 gap-y-0 mt-0.5 items-center">
                        {result.exif.focal_length && (
                          <span className="text-[9px] text-white/80">
                            {result.exif.focal_length}mm
                          </span>
                        )}
                        {result.exif.aperture && (
                          <span className="text-[9px] text-white/80">
                            f/{result.exif.aperture}
                          </span>
                        )}
                        {result.exif.iso && (
                          <span className="text-[9px] text-white/80">
                            ISO {result.exif.iso}
                          </span>
                        )}
                        {result.exif.exposure_time && (
                          <span className="text-[9px] text-white/80">
                            {formatExposureTimeGrid(result.exif.exposure_time)}
                          </span>
                        )}
                      </div>
                      {result.exif.datetime_original && (
                        <p className="text-[9px] text-white/70 mt-0.5">
                          {result.exif.datetime_original.split('T')[0]}
                        </p>
                      )}
                    </div>
                    {selectedImages.has(result.path) && (
                      <div className="absolute top-2 right-2 w-5 h-5 bg-primary rounded-full flex items-center justify-center">
                        <span className="text-primary-foreground text-xs">✓</span>
                      </div>
                    )}
                  </div>
                )}
              />
            ) : (
              <VirtualizedList
                items={filteredResults}
                renderItem={(result, index) => (
                  <div
                    key={result.path}
                    className={`flex items-center gap-3 p-2 rounded cursor-pointer transition-colors ${
                      detailMode === 'detailed' ? 'min-h-[60px]' : 'h-12'
                    } ${
                      selectedImages.has(result.path) ? 'bg-primary/10' : 'hover:bg-muted'
                    }`}
                    onClick={(e) => useAppStore.getState().toggleImageSelection(result.path, index, e.shiftKey, e.ctrlKey || e.metaKey)}
                    onDoubleClick={() => setSelectedDetailImage(result)}
                  >
                    <input
                      type="checkbox"
                      checked={selectedImages.has(result.path)}
                      onChange={() => {}}
                      className="w-4 h-4"
                    />
                    <div className="flex-1 min-w-0">
                      <p className="text-sm truncate">{result.path.split(/[/\\]/).pop()}</p>
                      {detailMode === 'detailed' ? (
                        <div className="flex flex-wrap gap-x-3 gap-y-0.5 mt-1">
                          {result.exif.make && (
                            <span className="text-xs text-muted-foreground">
                              {formatCamera(result.exif)}
                            </span>
                          )}
                          {result.exif.lens_model && (
                            <span className="text-xs text-muted-foreground">{result.exif.lens_model}</span>
                          )}
                          {result.exif.focal_length && (
                            <span className="text-xs text-muted-foreground">{result.exif.focal_length}mm</span>
                          )}
                          {result.exif.aperture && (
                            <span className="text-xs text-muted-foreground">f/{result.exif.aperture}</span>
                          )}
                          {result.exif.iso && (
                            <span className="text-xs text-muted-foreground">ISO {result.exif.iso}</span>
                          )}
                          {result.exif.datetime_original && (
                            <span className="text-xs text-muted-foreground">{result.exif.datetime_original.split('T')[0]}</span>
                          )}
                          <span className="text-xs text-muted-foreground">{(result.file_size / 1024).toFixed(0)}KB</span>
                        </div>
                      ) : (
                        <p className="text-xs text-muted-foreground truncate">{result.path}</p>
                      )}
                    </div>
                    {!detailMode || detailMode === 'simple' ? (
                      <div className="text-xs text-muted-foreground">
                        {result.exif.make && <span>{result.exif.make} </span>}
                        {result.exif.model && <span>{result.exif.model}</span>}
                      </div>
                    ) : null}
                  </div>
                )}
              />
            )}
          </div>

          {/* Empty State */}
          {filteredResults.length === 0 && !isScanning && (
            <div className="flex flex-col items-center justify-center h-64 text-muted-foreground">
              <p className="text-lg mb-2">没有图片</p>
              <p className="text-sm">点击"选择文件夹"开始扫描</p>
            </div>
          )}

          {/* Loading State */}
          {isScanning && (
            <div className="flex flex-col items-center justify-center h-64">
              <div className="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin mb-4" />
              <p className="text-sm text-muted-foreground mb-4">扫描中...</p>
              <div className="w-64 bg-muted rounded-full h-2 mb-2">
                <div
                  className="bg-primary h-2 rounded-full transition-all duration-300"
                  style={{ width: `${scanProgress}%` }}
                />
              </div>
              <p className="text-xs text-muted-foreground">{scanProgress.toFixed(0)}%</p>
              <Button variant="outline" size="sm" className="mt-4" onClick={cancelScan}>
                取消扫描
              </Button>
            </div>
          )}
        </main>
      </div>

      {/* Image Detail Modal */}
      {selectedDetailImage && (
        <ImageDetail
          result={selectedDetailImage}
          onClose={() => setSelectedDetailImage(null)}
          onDelete={handleDeleteFromDetail}
        />
      )}

      <StatusBar />

      {/* Delete Confirmation Dialog */}
      {showDeleteConfirm && (
        <ConfirmDialog
          title="确认删除"
          message={`确定要将以下 ${pendingDeleteImages.length} 张图片移到回收站吗？`}
          images={pendingDeleteImages}
          onConfirm={confirmDelete}
          onCancel={cancelDelete}
        />
      )}
    </div>
  )
}

export default App

function formatExposureTimeGrid(time: number): string {
  if (time >= 1) {
    return `${time}s`
  }
  const denominator = Math.round(1 / time)
  return `1/${denominator}s`
}
