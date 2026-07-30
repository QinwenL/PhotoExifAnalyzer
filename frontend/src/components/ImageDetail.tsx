import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { useAppStore, type ScanResult } from '../store'
import { Button } from '@/components/ui/button'
import { formatCamera } from '@/lib/utils'

interface ImageDetailProps {
  result: ScanResult
  onClose: () => void
}

export function ImageDetail({ result, onClose }: ImageDetailProps) {
  const [imageUrl, setImageUrl] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const { toggleImageSelection, selectedImages } = useAppStore()

  useEffect(() => {
    const loadImage = async () => {
      try {
        setLoading(true)
        const data: string = await invoke('get_image_data', {
          path: result.path,
          maxSize: 1200,
        })
        setImageUrl(data)
      } catch (error) {
        console.error('Failed to load image:', error)
      } finally {
        setLoading(false)
      }
    }

    loadImage()
  }, [result.path])

  const handleDelete = async () => {
    const confirmed = window.confirm('确定要将此图片移到回收站吗？')
    if (confirmed) {
      await invoke('delete_image', { path: result.path })
      onClose()
    }
  }

  const isSelected = selectedImages.has(result.path)

  return (
    <div className="fixed inset-0 z-50 bg-black/80 flex items-center justify-center p-4">
      <div className="bg-background rounded-lg max-w-6xl w-full max-h-[90vh] flex overflow-hidden">
        {/* Image Preview */}
        <div className="flex-1 bg-muted flex items-center justify-center min-h-[400px]">
          {loading ? (
            <div className="w-8 h-8 border-2 border-primary border-t-transparent rounded-full animate-spin" />
          ) : imageUrl ? (
            <img
              src={imageUrl}
              alt={result.path.split(/[/\\]/).pop()}
              className="max-w-full max-h-[80vh] object-contain"
            />
          ) : (
            <p className="text-muted-foreground">无法加载图片</p>
          )}
        </div>

        {/* EXIF Info Panel */}
        <div className="w-80 border-l border-border overflow-y-auto">
          <div className="p-4">
            <div className="flex items-center justify-between mb-4">
              <h3 className="font-semibold truncate" title={result.path}>
                {result.path.split(/[/\\]/).pop()}
              </h3>
              <Button variant="ghost" size="sm" onClick={onClose}>
                ✕
              </Button>
            </div>

            {/* File Info */}
            <div className="text-xs text-muted-foreground mb-4 break-all">
              {result.path}
            </div>

            {/* Action Buttons */}
            <div className="flex gap-2 mb-4">
              <Button
                size="sm"
                variant={isSelected ? 'default' : 'outline'}
                onClick={() => toggleImageSelection(result.path, 0, false, false)}
              >
                {isSelected ? '已选中' : '选择'}
              </Button>
              <Button size="sm" variant="destructive" onClick={handleDelete}>
                删除
              </Button>
            </div>

            {/* EXIF Data */}
            <div className="space-y-3">
              <h4 className="text-sm font-medium text-muted-foreground">EXIF 信息</h4>

              <ExifRow label="相机" value={formatCamera(result.exif)} />
              <ExifRow label="镜头" value={result.exif.lens_model} />
              <ExifRow
                label="焦距"
                value={result.exif.focal_length ? `${result.exif.focal_length}mm` : undefined}
              />
              <ExifRow
                label="光圈"
                value={result.exif.aperture ? `f/${result.exif.aperture}` : undefined}
              />
              <ExifRow
                label="ISO"
                value={result.exif.iso?.toString()}
              />
              <ExifRow
                label="快门"
                value={formatExposureTime(result.exif.exposure_time)}
              />
              <ExifRow label="曝光程序" value={result.exif.exposure_program} />
              <ExifRow label="测光模式" value={result.exif.metering_mode} />
              <ExifRow
                label="闪光灯"
                value={result.exif.flash !== undefined ? (result.exif.flash ? '是' : '否') : undefined}
              />
              <ExifRow label="白平衡" value={result.exif.white_balance} />
              <ExifRow
                label="尺寸"
                value={
                  result.exif.image_width && result.exif.image_height
                    ? `${result.exif.image_width} × ${result.exif.image_height}`
                    : undefined
                }
              />
              <ExifRow label="拍摄时间" value={result.exif.datetime_original} />
              <ExifRow
                label="GPS"
                value={formatGPS(result.exif.gps_latitude, result.exif.gps_longitude)}
              />
            </div>

            {/* File Size */}
            <div className="mt-4 pt-4 border-t border-border">
              <ExifRow label="文件大小" value={formatFileSize(result.file_size)} />
            </div>

            {/* Error */}
            {result.error && (
              <div className="mt-4 p-2 bg-destructive/10 text-destructive text-xs rounded">
                {result.error}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

function ExifRow({ label, value }: { label: string; value?: string }) {
  if (!value) return null

  return (
    <div className="flex justify-between text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right truncate max-w-[180px]" title={value}>
        {value}
      </span>
    </div>
  )
}

function formatExposureTime(time?: number): string | undefined {
  if (!time) return undefined

  if (time >= 1) {
    return `${time}s`
  }

  const denominator = Math.round(1 / time)
  return `1/${denominator}s`
}

function formatGPS(lat?: number, lon?: number): string | undefined {
  if (lat === undefined || lon === undefined) return undefined
  return `${lat.toFixed(6)}, ${lon.toFixed(6)}`
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
