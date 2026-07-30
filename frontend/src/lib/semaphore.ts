export class Semaphore {
  private available: number
  private readonly max: number
  private readonly waiters: Array<() => void> = []

  constructor(max: number) {
    this.max = max
    this.available = max
  }

  /** Number of permits currently available (mainly for tests). */
  getAvailableForTest(): number {
    return this.available
  }

  /** Reset available permits to max and drop all pending waiters (for tests only). */
  resetForTest(): void {
    this.waiters.length = 0
    this.available = this.max
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

// Matches backend MAX_IO_THREADS (4); keep in sync with
// frontend/src-tauri/src/exif/scanner.rs so the frontend concurrency
// ceiling matches the parallelism the backend is tuned for.
const THUMBNAIL_CONCURRENCY = 4
let thumbnailSlots: Semaphore = createThumbnailSemaphore(THUMBNAIL_CONCURRENCY)

export function createThumbnailSemaphore(max?: number): Semaphore {
  return new Semaphore(max ?? THUMBNAIL_CONCURRENCY)
}

/**
 * Reset the module-level semaphore singleton used by `withThumbnailSlot`.
 * Intended for test-suite isolation only (vitest beforeEach/afterEach).
 * Must NOT be called in application code.
 */
export function _resetThumbnailSemaphoreForTests(): void {
  thumbnailSlots = createThumbnailSemaphore(THUMBNAIL_CONCURRENCY)
}

export async function withThumbnailSlot<T>(fn: () => Promise<T>): Promise<T> {
  const release = await thumbnailSlots.acquire()
  try {
    return await fn()
  } finally {
    release()
  }
}
