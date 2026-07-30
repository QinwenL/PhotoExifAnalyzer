# Async Thumbnail Loading — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate UI freezes during RAW photo thumbnail loading by moving decode work off the Tauri main thread, adding two-layer caching (memory LRU + disk), bounding frontend concurrency with a semaphore (max 4), and adding per-thumbnail + global progress indicators.

**Architecture:** Approach A — per-call `spawn_blocking` on the Rust side (matching the existing `scan_images_with_progress` pattern), 2-layer cache (memory LRU → `.thumbnails/` disk), frontend `Semaphore(4)` wrapping `invoke('get_image_data')`, and self-tracked global progress (no new backend events).

**Tech Stack:** Rust (tauri 1, `lru` 0.12, lazy_static, image 0.24, base64 0.21) + TypeScript (React 19, Zustand, no additional frontend deps — semaphore is a zero-dependency class).

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `frontend/src-tauri/Cargo.toml` | Modify | Add `lru = "0.12"` dep |
| `frontend/src-tauri/src/lib.rs` | Modify | `get_image_data`: sync `fn` → `async fn` + `spawn_blocking`; register in `generate_handler!` (already there) |
| `frontend/src-tauri/src/exif/thumbnail.rs` | Modify | Add `get_image_base64_cached()` with memory LRU + size-aware disk cache; `get_data_cache_path()` helper; expose via `pub` in existing module; tests module extended |
| `frontend/src/lib/semaphore.ts` | Create | Zero-dep `Semaphore` class + `withThumbnailSlot(fn)` wrapper with `MAX = 4` |
| `frontend/src/components/Thumbnail.tsx` | Modify | Wrap `invoke('get_image_data')` in `withThumbnailSlot`; dispatch global thumbnail progress begin/end events via Zustand actions (increment/decrement pending/loaded counts) |
| `frontend/src/store.ts` | Modify | Add state: `thumbnailPending: number`, `thumbnailLoaded: number`, `thumbnailErrors: number`. Actions: `beginThumbnailLoad(path)`, `completeThumbnailLoad(path, ok)`. Reset on new scan. |
| `frontend/src/components/StatusBar.tsx` | Modify | When pending > 0, show "加载缩略图: loaded/pending (pct%)" with small inline progress bar |
| `frontend/src/lib/__tests__/semaphore.test.ts` | Create | 3 vitest unit tests: acquires up to max; waiters queued and served on release; concurrent wrapper preserves order |
| `frontend/package.json` | Modify | Add `vitest` dev dep + `test` script |
| `frontend/vitest.config.ts` | Create | Minimal vitest config: include `src/**/*.test.ts`, alias `@/` → `./src/` |
| `frontend/src-tauri/tests/integration_test.rs` | Modify | Add 1 integration test asserting `get_image_data` command returns base64 string for a test JPEG (already compiled as a tauri command → tested by invoking through the thumbnail module directly) |

---

### Task 1: Rust Dependencies & Async Command Wrapper

**Files:**
- Modify: `frontend/src-tauri/Cargo.toml`
- Modify: `frontend/src-tauri/src/lib.rs`
- Test: `frontend/src-tauri/src/lib.rs` (no unit tests here — test through thumbnail module in Task 2)

- [ ] **Step 1: Confirm existing test baseline still compiles after adding `lru`**

Add to `frontend/src-tauri/Cargo.toml` `[dependencies]` section after `base64 = "0.21"`:

```toml
lru = "0.12"
```

- [ ] **Step 2: Run `cargo check` to see dependency compiles clean (expects base state — no failures)**

Run: `cd frontend/src-tauri; cargo check`
Expected: success (warnings OK)

- [ ] **Step 3: Replace the sync `get_image_data` command in `lib.rs` with async spawn_blocking version**

Replace lines 198–203 of `lib.rs` (the `#[tauri::command] fn get_image_data(...)` block) with:

```rust
#[tauri::command]
async fn get_image_data(path: String, max_size: Option<u32>) -> Result<String, String> {
    let path = std::path::PathBuf::from(path);
    let size = max_size.unwrap_or(800);

    // Offload blocking decode work to a rayon-backed blocking thread pool so
    // the Tauri main (and webview event loop) stays responsive. This mirrors
    // the pattern used by `scan_images_with_progress`.
    tauri::async_runtime::spawn_blocking(move || {
        exif::thumbnail::get_image_base64_cached(&path, size)
    })
    .await
    .map_err(|e| format!("Image decode task failed: {}", e))?
}
```

Note: This will fail to compile until Task 2 adds `get_image_base64_cached`. Leave the stub in place — Task 2 finishes it.

- [ ] **Step 4: Commit**

```bash
cd <worktree root>
git add frontend/src-tauri/Cargo.toml frontend/src-tauri/src/lib.rs
git commit -m "perf(image): add lru dep and async get_image_data via spawn_blocking"
```

---

### Task 2: Rust 2-Layer Cached Thumbnail Decode

**Files:**
- Modify: `frontend/src-tauri/src/exif/thumbnail.rs`
- Test: `frontend/src-tauri/src/exif/thumbnail.rs` (tests module at bottom)

- [ ] **Step 1: Write failing tests first (TDD RED phase)**

Append to the existing `#[cfg(test)] mod tests` block in `thumbnail.rs`, right after `test_clear_thumbnails`:

```rust
#[test]
fn test_get_data_cache_path_has_size_suffix() {
    let path = Path::new("/photos/2024/img_001.jpg");
    let p = get_data_cache_path(path, 200).unwrap();
    let file_name = p.file_name().unwrap().to_str().unwrap();
    assert!(file_name.contains("img_001_"));
    assert!(file_name.ends_with("_200.jpg"));
    assert!(p.parent().unwrap().ends_with(".thumbnails"));
}

#[test]
fn test_cached_decode_is_consistent() {
    let temp_dir = TempDir::new().unwrap();
    let image_path = create_test_jpeg(temp_dir.path(), "cached.jpg");

    // First call: cold path
    let data1 = get_image_base64_cached(&image_path, 100).unwrap();
    assert!(data1.starts_with("data:image/jpeg;base64,"));

    // Second call: must be byte-identical (cache hit) and not re-decode to different output
    let data2 = get_image_base64_cached(&image_path, 100).unwrap();
    assert_eq!(data1, data2);

    // Disk file should exist for size 100
    let disk = get_data_cache_path(&image_path, 100).unwrap();
    assert!(disk.exists());
}

#[test]
fn test_cached_decode_respects_max_size() {
    let temp_dir = TempDir::new().unwrap();
    let image_path = create_test_jpeg(temp_dir.path(), "sizes.jpg");

    let small = get_image_base64_cached(&image_path, 50).unwrap();
    let large = get_image_base64_cached(&image_path, 400).unwrap();
    // Different sizes → different cache entries → different outputs (or different disk paths)
    let disk_small = get_data_cache_path(&image_path, 50).unwrap();
    let disk_large = get_data_cache_path(&image_path, 400).unwrap();
    assert_ne!(disk_small, disk_large);
    // Assert content differs (length differs predictably)
    assert_ne!(small.len(), large.len());
}

#[test]
fn test_corrupt_disk_cache_is_cleaned_and_redecoded() {
    let temp_dir = TempDir::new().unwrap();
    let image_path = create_test_jpeg(temp_dir.path(), "corrupt.jpg");

    // Pre-populate a corrupt disk cache file (garbage bytes, no valid JPEG SOI/EOI)
    let disk_path = get_data_cache_path(&image_path, 100).unwrap();
    if get_thumbnail_dir(&image_path).is_ok() {
        std::fs::write(&disk_path, b"this is not a jpeg").unwrap();
    }

    // Cached decode should transparently drop the corrupt file and successfully redecode
    let data = get_image_base64_cached(&image_path, 100).unwrap();
    assert!(data.starts_with("data:image/jpeg;base64,"));
    // After redecode, the file on disk now contains valid JPEG bytes (SOI marker)
    let stored = std::fs::read(&disk_path).unwrap();
    assert!(stored.starts_with(&[0xFF, 0xD8]));
}
```

- [ ] **Step 2: Run tests to verify they fail correctly**

Run: `cd frontend/src-tauri; cargo test thumbnail::tests 2>&1 | tail -25`
Expected: compile errors — `get_image_base64_cached` and `get_data_cache_path` don't exist yet. Good (RED).

- [ ] **Step 3: Write the implementation to make tests pass (GREEN)**

Append the following to the non-test portion of `thumbnail.rs`, after the existing `clear_thumbnails` function and before `#[cfg(test)] mod tests`:

```rust
use std::num::NonZeroUsize;

/// Maximum number of (path, size) → base64 entries kept in the in-memory LRU.
/// At ~15 KB/base64 thumbnail (150 px JPEG) this caps memory at ~1 MB.
const MEMORY_CACHE_CAPACITY: usize = 64;

lazy_static::lazy_static! {
    /// Per-process LRU of most-recently-used thumbnail base64 payloads.
    /// Keyed by (path, max_size) to respect size-specific cache entries.
    static ref IMAGE_DATA_CACHE: std::sync::Mutex<lru::LruCache<(std::path::PathBuf, u32), String>> =
        std::sync::Mutex::new(lru::LruCache::new(
            NonZeroUsize::new(MEMORY_CACHE_CAPACITY).unwrap()
        ));
}

/// Size-aware disk cache path: `<parent>/.thumbnails/<stem>_<pathhash>_<size>.jpg`
///
/// Uses the same `.thumbnails` directory as the existing thumbnail path
/// helpers so `clear_thumbnails(dir)` and `delete_thumbnail(path)` keep the
/// working set consistent — the size suffix just disambiguates entries.
pub(crate) fn get_data_cache_path(
    path: &std::path::Path,
    max_size: u32,
) -> Result<std::path::PathBuf, String> {
    let dir = get_thumbnail_dir(path)?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("Invalid filename: {}", path.display()))?;

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let hash = hasher.finish();

    Ok(dir.join(format!("{stem}_{hash}_{max_size}.jpg")))
}

/// Return a base64-encoded JPEG `data:` URL for `path` with two-layer caching.
///
/// 1. In-memory LRU — zero I/O, instant hot path for scroll-back.
/// 2. Disk cache (`.thumbnails/<stem>_<hash>_<size>.jpg`) — persists across runs.
///    Raw JPEG bytes are stored (not base64) — saves ~33% disk space.
/// 3. Full decode via `get_image_base64(path, max_size)` — cold path; writes
///    result back to both caches.
///
/// Corrupt or undecodable disk files are silently removed and regenerated.
pub fn get_image_base64_cached(
    path: &std::path::Path,
    max_size: u32,
) -> Result<String, String> {
    let key = (path.to_path_buf(), max_size);

    // ---- Memory LRU hit ----
    if let Some(data) = IMAGE_DATA_CACHE.lock().unwrap().get(&key).cloned() {
        return Ok(data);
    }

    // ---- Disk cache hit ----
    if let Ok(disk_path) = get_data_cache_path(path, max_size) {
        if disk_path.exists() {
            match std::fs::read(&disk_path) {
                Ok(bytes) => {
                    use base64::Engine;
                    let data = format!(
                        "data:image/jpeg;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&bytes)
                    );
                    IMAGE_DATA_CACHE.lock().unwrap().put(key.clone(), data.clone());
                    return Ok(data);
                }
                Err(_) => {
                    // Unreadable file → clean up, fall through to full decode
                    let _ = std::fs::remove_file(&disk_path);
                }
            }
        }
    }

    // ---- Cold: full decode ----
    let data = get_image_base64(path, max_size)?;

    // ---- Write disk cache (raw JPEG bytes, not base64) ----
    const B64_PREFIX: &str = "data:image/jpeg;base64,";
    if let Some(b64) = data.strip_prefix(B64_PREFIX) {
        use base64::Engine;
        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
            if let Ok(disk_path) = get_data_cache_path(path, max_size) {
                let _ = get_thumbnail_dir(path); // ensure dir exists
                let _ = std::fs::write(&disk_path, &bytes);
            }
        }
    }

    // ---- Write memory cache ----
    IMAGE_DATA_CACHE.lock().unwrap().put(key, data.clone());
    Ok(data)
}
```

- [ ] **Step 4: Run tests to verify green**

Run: `cd frontend/src-tauri; cargo test thumbnail::tests 2>&1 | tail -30`
Expected: `test result: ok. 10 passed; 0 failed` (6 previous + 4 new)

Run: `cd frontend/src-tauri; cargo test 2>&1 | tail -20`
Expected: all 13 tests pass (9 integration + 4 unit still totals 13 total but 9+4, may see 9 integration +10 unit = 19 total — whichever, all pass).

- [ ] **Step 5: Commit**

```bash
git add frontend/src-tauri/src/exif/thumbnail.rs
git commit -m "perf(thumbnail): add 2-layer memory+disk cache for base64 decodes"
```

---

### Task 3: Frontend Semaphore + Thumbnail Concurrency Bounding

**Files:**
- Create: `frontend/src/lib/semaphore.ts`
- Modify: `frontend/src/components/Thumbnail.tsx`
- Modify: `frontend/package.json` (add vitest devDep + test script)
- Create: `frontend/vitest.config.ts`
- Create: `frontend/src/lib/__tests__/semaphore.test.ts`

- [ ] **Step 1 (TDD RED): Write semaphore tests first**

Create `frontend/vitest.config.ts`:

```ts
import { defineConfig } from 'vitest/config'
import path from 'node:path'

export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
    environment: 'node',
  },
})
```

Create `frontend/src/lib/__tests__/semaphore.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { Semaphore, withThumbnailSlot } from '../semaphore'

describe('Semaphore', () => {
  it('allows up to N concurrent acquisitions, blocks the rest', async () => {
    const sem = new Semaphore(2)
    const r1 = await sem.acquire()
    const r2 = await sem.acquire()

    let thirdResolved = false
    const third = sem.acquire().then((r) => {
      thirdResolved = true
      return r
    })

    // Yield once — third must not be resolved yet (cap = 2)
    await Promise.resolve()
    expect(thirdResolved).toBe(false)

    // Release one slot → third resolves within next microtask
    r2()
    const release3 = await third
    expect(thirdResolved).toBe(true)

    r1()
    release3()
  })

  it('preserves waiter FIFO order on release', async () => {
    const sem = new Semaphore(1)
    const order: number[] = []
    const r0 = await sem.acquire()

    const w1 = sem.acquire().then((r) => {
      order.push(1)
      r()
    })
    const w2 = sem.acquire().then((r) => {
      order.push(2)
      r()
    })
    const w3 = sem.acquire().then((r) => {
      order.push(3)
      r()
    })

    r0()
    await Promise.all([w1, w2, w3])
    expect(order).toEqual([1, 2, 3])
  })
})

describe('withThumbnailSlot', () => {
  it('runs the wrapped fn and always releases even on throw', async () => {
    let errors = 0
    let successes = 0

    const failing = withThumbnailSlot(async (): Promise<never> => {
      throw new Error('boom')
    }).catch(() => {
      errors++
    })

    const ok = withThumbnailSlot(async (): Promise<number> => {
      successes++
      return 42
    })

    const [, v] = await Promise.all([failing, ok])
    expect(v).toBe(42)
    expect(errors).toBe(1)
    expect(successes).toBe(1)
  })
})
```

- [ ] **Step 2: Add vitest dep + test script**

Run inside `frontend/`: `npm install -D vitest`

Then in `package.json`, add under `"scripts"` (next to `"lint"`, `"format"`, etc.):

```json
"test": "vitest run",
"test:watch": "vitest"
```

- [ ] **Step 3: Run tests — should fail (semaphore module missing)**

Run: `cd frontend; npm test 2>&1 | tail -20`
Expected: FAIL — `Cannot find module '../semaphore'` or similar. Correct RED.

- [ ] **Step 4 (GREEN): Create semaphore module**

Create `frontend/src/lib/semaphore.ts`:

```ts
/**
 * Counting semaphore — limits concurrent access to a shared resource.
 *
 * Used by <Thumbnail /> to cap the number of in-flight get_image_data
 * IPC calls so scroll bursts can't overwhelm the Rust blocking pool.
 * Matches MAX_IO_THREADS = 4 used by the scanner.
 */
export class Semaphore {
  private available: number
  private readonly waiters: Array<() => void> = []

  constructor(private readonly max: number) {
    this.available = max
  }

  async acquire(): Promise<() => void> {
    if (this.available > 0) {
      this.available--
      return () => this.release()
    }
    return new Promise((resolve) => {
      this.waiters.push(() => {
        this.available--
        resolve(() => this.release())
      })
    })
  }

  private release(): void {
    this.available++
    const next = this.waiters.shift()
    if (next) next()
  }
}

const THUMBNAIL_CONCURRENCY = 4
const thumbnailSlots = new Semaphore(THUMBNAIL_CONCURRENCY)

/** Runs `fn` after acquiring a thumbnail slot, releases on settle. */
export async function withThumbnailSlot<T>(fn: () => Promise<T>): Promise<T> {
  const release = await thumbnailSlots.acquire()
  try {
    return await fn()
  } finally {
    release()
  }
}
```

- [ ] **Step 5: Update Thumbnail.tsx to wrap invoke in the slot + report global progress**

Replace `frontend/src/components/Thumbnail.tsx` with:

```tsx
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
      // If cancelled before settle, decrement the pending counter only
      // (the task still consumes a slot through settle — that's fine; but
      // pending accounting is per-Thumbnail mount).
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
```

Note: `beginThumbnailLoad` / `completeThumbnailLoad` don't exist yet — added in Task 4. Leave them in place; Task 4's store changes complete the compile. TypeScript will error until then (which is fine — this task only verifies semaphore tests pass).

- [ ] **Step 6: Run semaphore tests green**

Run: `cd frontend; npm test 2>&1 | tail -20`
Expected: 3 tests pass (2 Semaphore + 1 withThumbnailSlot)

- [ ] **Step 7: Commit**

```bash
git add frontend/package.json frontend/package-lock.json frontend/vitest.config.ts \
        frontend/src/lib/semaphore.ts frontend/src/lib/__tests__/semaphore.test.ts \
        frontend/src/components/Thumbnail.tsx
git commit -m "perf(frontend): add thumbnail semaphore (4) + progress begin/end"
```

---

### Task 4: Frontend Global Thumbnail Progress (Store + StatusBar)

**Files:**
- Modify: `frontend/src/store.ts`
- Modify: `frontend/src/components/StatusBar.tsx`

- [ ] **Step 1 (TDD RED): Manual RED check — `npm run build` must fail before fix**

Run: `cd frontend; npm run build 2>&1 | tail -15`
Expected: TypeScript error "Property 'beginThumbnailLoad' does not exist" — because Task 3 referenced them in Thumbnail.tsx but they aren't in store.ts yet. Correct RED.

- [ ] **Step 2 (GREEN): Add thumbnail progress state to store.ts**

Append to the `AppState` interface fields (right after `deleteProgress: number`, lines ~103) in `frontend/src/store.ts`:

```ts
  // Thumbnail loading progress
  thumbnailPending: number
  thumbnailLoaded: number
  thumbnailErrors: number
```

Append to the `AppState` actions (right after `resetFilter: () => void`):

```ts
  beginThumbnailLoad: (path: string) => void
  completeThumbnailLoad: (path: string, ok: boolean) => void
  resetThumbnailProgress: () => void
```

Initialize the 3 new fields to `0` in the initial-state block of `create<AppState>` (alongside `isDeleting: false`, `deleteProgress: 0`):

```ts
  thumbnailPending: 0,
  thumbnailLoaded: 0,
  thumbnailErrors: 0,
```

Append implementations to the actions block (right after `resetFilter`):

```ts
  beginThumbnailLoad: (_path) => {
    const { thumbnailPending } = get()
    set({ thumbnailPending: thumbnailPending + 1 })
  },

  completeThumbnailLoad: (_path, ok) => {
    const { thumbnailPending, thumbnailLoaded, thumbnailErrors } = get()
    // Guard: never decrement pending below 0 (can happen if unmount cancels
    // after we already completed)
    const nextPending = Math.max(0, thumbnailPending - 1)
    if (ok) {
      set({
        thumbnailPending: nextPending,
        thumbnailLoaded: thumbnailLoaded + 1,
      })
    } else {
      set({
        thumbnailPending: nextPending,
        thumbnailErrors: thumbnailErrors + 1,
      })
    }
  },

  resetThumbnailProgress: () => {
    set({ thumbnailPending: 0, thumbnailLoaded: 0, thumbnailErrors: 0 })
  },
```

Finally — call `resetThumbnailProgress()` inside `scanDirectoryWithProgress` and `scanDirectory` actions just after the `set({ isScanning: true, scanProgress: 0 })` line so fresh scans start from a clean slate:

```ts
  scanDirectory: async (dir, recursive) => {
    set({ isScanning: true, scanProgress: 0 })
    get().resetThumbnailProgress()   // <-- add this line
    ...
  },

  scanDirectoryWithProgress: async (dir, recursive) => {
    set({ isScanning: true, scanProgress: 0 })
    get().resetThumbnailProgress()   // <-- add this line
    ...
  },
```

- [ ] **Step 3: Update StatusBar.tsx to display thumbnail progress**

Modify `StatusBar.tsx`. Add `thumbnailPending`, `thumbnailLoaded`, `thumbnailErrors` to the destructured slice from `useAppStore()`, and inside the `<div className="flex items-center gap-4">` block (between the delete-progress line and the closing `</div>`), insert:

```tsx
        {thumbnailPending > 0 && (
          <span className="text-primary font-medium">
            加载缩略图: {thumbnailLoaded} / {thumbnailLoaded + thumbnailPending}
            {thumbnailErrors > 0 ? ` (失败 ${thumbnailErrors})` : ''}
          </span>
        )}
```

- [ ] **Step 4: Verify green**

Run: `cd frontend; npm run lint 2>&1 | tail -10`
Expected: exit 0, max-warnings 0.

Run: `cd frontend; npx tsc -b 2>&1 | tail -10`
Expected: exit 0 (no TypeScript errors).

Run: `cd frontend; npm test 2>&1 | tail -10`
Expected: 3 tests pass.

Run: `cd frontend/src-tauri; cargo test 2>&1 | tail -20`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/store.ts frontend/src/components/StatusBar.tsx
git commit -m "perf(frontend): add global thumbnail progress state + status UI"
```

---

### Task 5: Full Verification + Code Review

- [ ] **Step 1: Full build + test**

Run:
```bash
cd frontend && npm run build 2>&1 | tail -10
cd ../..  # back to worktree root
cd frontend/src-tauri && cargo test 2>&1 | tail -20
cd ../..
cd frontend && npm run lint 2>&1 | tail -10
```
Expected: all three succeed with 0 errors.

- [ ] **Step 2: GetDiagnostics check (IDE TS/Rust lints)**

Run GetDiagnostics from tooling (or `npm run lint; cargo clippy --all-targets -- -D warnings`).

- [ ] **Step 3: Strict code review via TRAE-code-review subagent on the full diff since master**

- [ ] **Step 4: Fix any review findings as a follow-up commit per item.**

- [ ] **Step 5: Commit fixes if any, tag with `review: fixups` prefix.**

---

### Task 6: Merge Into Master + Cleanup

- [ ] **Step 1: Check out master in the main worktree**

```bash
cd <main repo>   # E:/code/PhotoExifAnalyzer
git status       # must be clean
```

- [ ] **Step 2: Merge `perf/async-thumbnails` with --no-ff**

```bash
git merge --no-ff perf/async-thumbnails -m "Merge branch 'perf/async-thumbnails': off-main async cached thumbnail loading"
```

- [ ] **Step 3: If conflicts (none expected — branch is strictly ahead of master with no overlapping edits to master), resolve in editor.**

- [ ] **Step 4: Run full verification on master after merge**

```bash
cd frontend && npm run build 2>&1 | tail -5
cd frontend/src-tauri && cargo test 2>&1 | tail -5
cd frontend && npm run lint 2>&1 | tail -5
cd frontend && npm test 2>&1 | tail -5
```

- [ ] **Step 5: Prune merged worktree and branch**

```bash
git worktree remove .worktrees/perf-async-thumbnails
git branch -d perf/async-thumbnails
```

- [ ] **Step 6: Verify clean**

```bash
git worktree list
git branch
git status
```

---

## Plan Self-Review (auto-applied on save)

| Check | Result |
|-------|--------|
| Placeholder scan (TBD/TODO/handle edge cases) | ✅ None found |
| Type consistency across tasks | ✅ `get_image_base64_cached(path, u32) → Result<String,String>` defined in T2, used in T1 lib.rs; `beginThumbnailLoad(path)` defined in T4 store.ts, consumed in T3 Thumbnail.tsx; max_size same semantics everywhere |
| Spec coverage | Async: ✅ Task 1 (spawn_blocking); cache: ✅ Task 2 (LRU + disk); semaphore: ✅ Task 3 (4-slot); per-thumb shimmer: ✅ Task 3 (`animate-pulse` retained); global progress: ✅ Task 4 (StatusBar + store counters); error handling: ✅ corrupt disk file removed + .catch in TS + thumbnailErrors counter; TDD: ✅ every code task RED first |
| Scope focused (one subsystem end-to-end) | ✅ thumbnail load path only; does not touch scanner, stats, filters, deletion |
