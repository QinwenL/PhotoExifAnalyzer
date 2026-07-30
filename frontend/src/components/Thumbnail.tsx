import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { withThumbnailSlot } from '../lib/semaphore'
import { useAppStore } from '../store'

interface ThumbnailProps {
  path: string
  maxSize?: number
  className?: string
}

export function Thumbnail({ path, maxSize = 200, className = '' }: ThumbnailProps) {
  const [src, setSrc] = useState<string | null>(null)
  const [error, setError] = useState(false)
  const { beginThumbnailLoad, completeThumbnailLoad } = useAppStore()

  useEffect(() => {
    let cancelled = false

    beginThumbnailLoad(path)

    withThumbnailSlot(() => invoke<string>('get_image_data', { path, maxSize }))
      .then((data) => {
        if (!cancelled) setSrc(data)
        if (!cancelled) completeThumbnailLoad(path, true)
      })
      .catch(() => {
        if (!cancelled) {
          setError(true)
          completeThumbnailLoad(path, false)
        }
      })

    return () => {
      cancelled = true
      if (!src && !error) {
        completeThumbnailLoad(path, false)
      }
    }
  }, [path, maxSize, beginThumbnailLoad, completeThumbnailLoad, src, error])

  if (error) {
    return (
      <div className={`bg-muted flex items-center justify-center ${className}`}>
        <span className="text-[10px] text-muted-foreground truncate px-1">
          {path.split(/[/\\]/).pop()}
        </span>
      </div>
    )
  }

  if (!src) {
    return <div className={`bg-muted animate-pulse ${className}`} />
  }

  return (
    <img
      src={src}
      alt=""
      className={`object-cover w-full h-full ${className}`}
      loading="lazy"
    />
  )
}
