import { Button } from '@/components/ui/button'
import { Thumbnail } from './Thumbnail'
import { getFileName } from '@/lib/utils'
import type { ScanResult } from '../store'

interface ConfirmDialogProps {
  title: string
  message: string
  images: ScanResult[]
  maxPreview?: number
  onConfirm: () => void
  onCancel: () => void
}

export function ConfirmDialog({
  title,
  message,
  images,
  maxPreview = 8,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const previewImages = images.slice(0, maxPreview)
  const remaining = images.length - maxPreview

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div className="bg-background rounded-lg shadow-lg max-w-md w-full mx-4 overflow-hidden">
        {/* Header */}
        <div className="p-4 border-b border-border">
          <h3 className="text-lg font-semibold">{title}</h3>
        </div>

        {/* Body */}
        <div className="p-4">
          <p className="text-sm text-muted-foreground mb-4">{message}</p>

          {/* Image preview grid */}
          {images.length > 0 && (
            <div className="grid grid-cols-4 gap-2 mb-3">
              {previewImages.map((img) => (
                <div
                  key={img.path}
                  className="aspect-square bg-muted rounded flex items-center justify-center overflow-hidden"
                  title={getFileName(img.path)}
                >
                  <Thumbnail path={img.path} className="w-full h-full" />
                </div>
              ))}
            </div>
          )}

          {remaining > 0 && (
            <p className="text-xs text-muted-foreground text-center">
              ...还有 {remaining} 张图片
            </p>
          )}
        </div>

        {/* Footer */}
        <div className="flex justify-end gap-2 p-4 border-t border-border">
          <Button variant="outline" onClick={onCancel}>
            取消
          </Button>
          <Button variant="destructive" onClick={onConfirm}>
            移到回收站
          </Button>
        </div>
      </div>
    </div>
  )
}
