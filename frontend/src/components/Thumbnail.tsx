import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { withThumbnailSlot } from '../lib/semaphore'
import { getFileName } from '@/lib/utils'
import { useAppStore } from '../store'

interface ThumbnailProps {
  path: string
  // 200px is the default for UI grid tiles; override for large detail view.
  // Note: the Rust `get_image_data` default for `maxSize === undefined` is
  // 800, so passing `undefined` explicitly opts into the bigger backend size.
  maxSize?: number
  className?: string
}

export function Thumbnail({ path, maxSize = 200, className = '' }: ThumbnailProps) {
  const [src, setSrc] = useState<string | null>(null)
  const [error, setError] = useState(false)
  // Subscribe to `thumbnailEpoch` so this component re-renders (and thus
  // re-runs the load effect below) whenever the app rotates into a new
  // scan batch. We *cannot* rely on a scanResults swap alone to remount:
  // if the user cancels a scan and immediately re-scans the SAME directory,
  // `key={result.path}` in VirtualizedGrid will be identical and React will
  // reuse the component instance. Without an explicit epoch dependency,
  // still-loading thumbnails (src === null) would remain stuck in the
  // loading skeleton forever because effect deps `[path, maxSize]` haven't
  // changed — counters got zeroed by resetThumbnailProgress but no one
  // re-issued the `invoke`. Subscribing here is the minimal fix that
  // matches the counter-reset contract in `store.ts`.
  const thumbnailEpoch = useAppStore((s) => s.thumbnailEpoch)

  useEffect(() => {
    let cancelled = false
    let settled = false
    // `settled` is the single source of truth for whether the load has
    // finished (then/catch branch) and thus been accounted for in the
    // counters. Cleanup only needs to balance begin with a synthetic
    // complete if we were cancelled mid-flight *before* either branch
    // ran. See R-6 rationale: redundant `accountedComplete` was removed.
    const snapshot = useAppStore.getState()
    const epoch = snapshot.thumbnailEpoch
    const begin = snapshot.beginThumbnailLoad
    const complete = snapshot.completeThumbnailLoad

    begin(path, epoch)

    withThumbnailSlot(() => invoke<string>('get_image_data', { path, maxSize }))
      .then((data) => {
        if (cancelled || settled) return
        settled = true
        setSrc(data)
        complete(path, true, epoch)
      })
      .catch(() => {
        if (cancelled || settled) return
        settled = true
        setError(true)
        complete(path, false, epoch)
      })

    return () => {
      cancelled = true
      if (!settled) {
        complete(path, false, epoch)
      }
    }
    // Dependencies are intentionally `[path, maxSize, thumbnailEpoch]`.
    //  - begin/complete are read from `getState()` (Zustand store action
    //    refs are stable across the app's lifetime, so including them
    //    would be noise and — worse — would cause cascade re-runs if
    //    middleware ever swapped them out).
    //  - `epoch` is snapshot-captured at effect start; deps include
    //    `thumbnailEpoch` as a selector subscription so a new scan batch
    //    re-runs the effect for any still-skeleton tile.
  }, [path, maxSize, thumbnailEpoch])

  if (error) {
    return (
      <div className={`bg-muted flex items-center justify-center ${className}`}>
        <span className="text-[10px] text-muted-foreground truncate px-1">
          {getFileName(path)}
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
