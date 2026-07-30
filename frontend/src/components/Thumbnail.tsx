import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'

interface ThumbnailProps {
  path: string
  maxSize?: number
  className?: string
}

export function Thumbnail({ path, maxSize = 200, className = '' }: ThumbnailProps) {
  const [src, setSrc] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false

    invoke<string>('get_image_data', { path, maxSize })
      .then((data) => {
        if (!cancelled) {
          setError(null)
          setSrc(data)
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err)
          console.error('[Thumbnail] Failed to load:', path, message)
          setError(message)
        }
      })

    return () => {
      cancelled = true
    }
  }, [path, maxSize])

  const filename = path.split(/[/\\]/).pop() ?? path

  if (error) {
    const isHeicError = error.toLowerCase().includes('heic') || error.toLowerCase().includes('heif')
    const isRawError = error.toLowerCase().includes('raw')
    const errorMessage = isHeicError
      ? 'HEIC 格式不支持预览'
      : isRawError
        ? 'RAW 格式预览不可用'
        : '加载失败'

    return (
      <div
        className={`bg-muted flex flex-col items-center justify-center ${className}`}
        title={`${filename}: ${error}`}
      >
        <span className="text-[10px] text-muted-foreground truncate px-1 max-w-full">
          {filename}
        </span>
        <span className="text-[8px] text-destructive truncate px-1 mt-0.5 max-w-full">
          {errorMessage}
        </span>
      </div>
    )
  }

  if (!src) {
    return (
      <div className={`bg-muted animate-pulse ${className}`} title={filename} />
    )
  }

  return (
    <img
      src={src}
      alt={filename}
      className={`object-cover w-full h-full ${className}`}
      loading="lazy"
    />
  )
}