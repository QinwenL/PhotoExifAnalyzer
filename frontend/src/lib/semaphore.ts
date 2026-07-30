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

export async function withThumbnailSlot<T>(fn: () => Promise<T>): Promise<T> {
  const release = await thumbnailSlots.acquire()
  try {
    return await fn()
  } finally {
    release()
  }
}
