import { useAppStore } from '../store'

export function StatusBar() {
  const {
    scanResults,
    filteredResults,
    selectedImages,
    selectedDirectory,
    isScanning,
    scanProgress,
    isDeleting,
    deleteProgress,
  } = useAppStore()

  return (
    <div className="fixed bottom-0 left-0 right-0 border-t border-border bg-background/95 backdrop-blur px-4 py-1.5 flex items-center justify-between text-xs text-muted-foreground z-40">
      <div className="flex items-center gap-4">
        <span>总计: {scanResults.length} 张</span>
        <span>显示: {filteredResults.length} 张</span>
        {selectedImages.size > 0 && (
          <span className="text-primary font-medium">已选: {selectedImages.size} 张</span>
        )}
        {isScanning && (
          <span className="text-primary font-medium">扫描中: {scanProgress.toFixed(0)}%</span>
        )}
        {isDeleting && (
          <span className="text-destructive font-medium">删除中: {deleteProgress.toFixed(0)}%</span>
        )}
      </div>
      <div className="flex items-center gap-4">
        {filteredResults.length !== scanResults.length && scanResults.length > 0 && (
          <button
            className="text-primary hover:underline cursor-pointer"
            onClick={() => useAppStore.getState().resetFilter()}
          >
            清除筛选
          </button>
        )}
        {selectedDirectory && (
          <span className="truncate max-w-[300px]" title={selectedDirectory}>
            {selectedDirectory}
          </span>
        )}
      </div>
    </div>
  )
}
