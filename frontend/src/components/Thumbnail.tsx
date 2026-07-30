import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { withThumbnailSlot } from '../lib/semaphore'
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

  useEffect(() => {
    let cancelled = false
    let settled = false
    // Per-effect accounting: ensure begin/complete are called exactly once
    // each even under React 19 StrictMode double-mount. Cleanup always
    // completes on our behalf if we haven't already settled via then/catch.
    // (The `settled` guard prevents double-completion in the same run.)
    let accountedComplete = false
    const snapshot = useAppStore.getState()
    const epoch = snapshot.thumbnailEpoch
    const begin = snapshot.beginThumbnailLoad
    const complete = snapshot.completeThumbnailLoad

    begin(path, epoch)

    withThumbnailSlot(() => invoke<string>('get_image_data', { path, maxSize }))
      .then((data) => {
        if (cancelled || settled) return
        settled = true
        accountedComplete = true
        setSrc(data)
        complete(path, true, epoch)
      })
      .catch(() => {
        if (cancelled || settled) return
        settled = true
        accountedComplete = true
        setError(true)
        complete(path, false, epoch)
      })

    return () => {
      cancelled = true
      if (!settled) {
        settled = true
        if (!accountedComplete) {
          accountedComplete = true
          complete(path, false, epoch)
        }
      }
    }
    // Intentionally only `[path, maxSize]`:
    //  - begin/complete are read from `getState()` (Zustand store action
    //    refs are stable across the app's lifetime, so including them would
    //    be noise and — worse — would cause cascade re-runs if middleware
    //    ever swapped them out).
    //  - `epoch` is snapshot-captured at effect start; on a genuine epoch
    //    change the Thumbnail parent will unmount/remount anyway via the
    //    `scanResults` swap, so we don't need it in the deps array.
  }, [path, maxSize])

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
