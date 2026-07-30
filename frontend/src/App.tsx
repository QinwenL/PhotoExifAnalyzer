import { open } from '@tauri-apps/api/dialog'
import { useAppStore } from './store'
import { Button } from '@/components/ui/button'

function App() {
  const {
    isScanning,
    filteredResults,
    cameraStats,
    lensStats,
    viewMode,
    selectedImages,
    setSelectedDirectory,
    scanDirectory,
    deleteSelectedImages,
    setViewMode,
    selectAllImages,
    clearSelection,
  } = useAppStore()

  const handleSelectDirectory = async () => {
    const selected = await open({ directory: true, multiple: false })
    if (selected && typeof selected === 'string') {
      setSelectedDirectory(selected)
      await scanDirectory(selected, true)
    }
  }

  const handleDelete = async () => {
    if (selectedImages.size === 0) return
    const confirmed = window.confirm(`确定要将 ${selectedImages.size} 张图片移到回收站吗？`)
    if (confirmed) {
      await deleteSelectedImages()
    }
  }

  return (
    <div className="min-h-screen bg-background text-foreground">
      {/* Header */}
      <header className="border-b border-border p-4">
        <div className="flex items-center justify-between">
          <h1 className="text-2xl font-bold">PhotoExifAnalyzer</h1>
          <div className="flex gap-2">
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

      {/* Main Content */}
      <div className="flex h-[calc(100vh-65px)]">
        {/* Sidebar - Statistics */}
        <aside className="w-64 border-r border-border p-4 overflow-y-auto">
          <h2 className="text-lg font-semibold mb-4">统计信息</h2>

          {cameraStats && cameraStats.cameras.length > 0 && (
            <div className="mb-6">
              <h3 className="text-sm font-medium text-muted-foreground mb-2">相机</h3>
              <div className="space-y-1">
                {cameraStats.cameras.slice(0, 10).map((camera) => (
                  <div key={camera.name} className="flex justify-between text-sm">
                    <span className="truncate">{camera.name}</span>
                    <span className="text-muted-foreground">{camera.count}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {lensStats && lensStats.lenses.length > 0 && (
            <div className="mb-6">
              <h3 className="text-sm font-medium text-muted-foreground mb-2">镜头</h3>
              <div className="space-y-1">
                {lensStats.lenses.slice(0, 10).map((lens) => (
                  <div key={lens.name} className="flex justify-between text-sm">
                    <span className="truncate">{lens.name}</span>
                    <span className="text-muted-foreground">{lens.count}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {!cameraStats && !isScanning && (
            <p className="text-sm text-muted-foreground">选择一个文件夹开始分析</p>
          )}
        </aside>

        {/* Main Area */}
        <main className="flex-1 overflow-y-auto p-4">
          {/* Toolbar */}
          <div className="flex items-center justify-between mb-4">
            <span className="text-sm text-muted-foreground">{filteredResults.length} 张图片</span>
            <div className="flex gap-2">
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
              {filteredResults.length > 0 && (
                <>
                  <Button variant="outline" size="sm" onClick={selectAllImages}>
                    全选
                  </Button>
                  <Button variant="outline" size="sm" onClick={clearSelection}>
                    取消选择
                  </Button>
                </>
              )}
            </div>
          </div>

          {/* Image Grid/List */}
          {viewMode === 'grid' ? (
            <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-2">
              {filteredResults.map((result) => (
                <div
                  key={result.path}
                  className={`relative aspect-square border rounded-lg overflow-hidden cursor-pointer transition-all ${
                    selectedImages.has(result.path)
                      ? 'ring-2 ring-primary ring-offset-2'
                      : 'hover:ring-2 hover:ring-primary/50'
                  }`}
                  onClick={() => useAppStore.getState().toggleImageSelection(result.path)}
                >
                  <div className="absolute inset-0 bg-muted flex items-center justify-center">
                    <span className="text-xs text-muted-foreground truncate px-2">
                      {result.path.split(/[/\\]/).pop()}
                    </span>
                  </div>
                  {selectedImages.has(result.path) && (
                    <div className="absolute top-2 right-2 w-5 h-5 bg-primary rounded-full flex items-center justify-center">
                      <span className="text-primary-foreground text-xs">✓</span>
                    </div>
                  )}
                </div>
              ))}
            </div>
          ) : (
            <div className="space-y-1">
              {filteredResults.map((result) => (
                <div
                  key={result.path}
                  className={`flex items-center gap-3 p-2 rounded cursor-pointer transition-colors ${
                    selectedImages.has(result.path) ? 'bg-primary/10' : 'hover:bg-muted'
                  }`}
                  onClick={() => useAppStore.getState().toggleImageSelection(result.path)}
                >
                  <input
                    type="checkbox"
                    checked={selectedImages.has(result.path)}
                    onChange={() => {}}
                    className="w-4 h-4"
                  />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm truncate">{result.path.split(/[/\\]/).pop()}</p>
                    <p className="text-xs text-muted-foreground truncate">{result.path}</p>
                  </div>
                  <div className="text-xs text-muted-foreground">
                    {result.exif.make && <span>{result.exif.make} </span>}
                    {result.exif.model && <span>{result.exif.model}</span>}
                  </div>
                </div>
              ))}
            </div>
          )}

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
              <p className="text-sm text-muted-foreground">扫描中...</p>
            </div>
          )}
        </main>
      </div>
    </div>
  )
}

export default App
