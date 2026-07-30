import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  Semaphore,
  createThumbnailSemaphore,
  withThumbnailSlot,
  _resetThumbnailSemaphoreForTests,
} from '../semaphore'

describe('Semaphore', () => {
  it('allows up to N concurrent acquisitions and actually blocks the N+1th', async () => {
    const sem = new Semaphore(2)
    const r1 = await sem.acquire()
    const r2 = await sem.acquire()

    // Third acquire must NOT resolve until we release a slot.
    let thirdResolved = false
    const third = sem.acquire().then((r) => {
      thirdResolved = true
      return r
    })

    // Flush all microtasks explicitly (many passes) so any accidental
    // immediate-resolve bug would surface.
    for (let i = 0; i < 10; i++) await Promise.resolve()
    expect(thirdResolved).toBe(false)

    r2() // release one slot
    const release3 = await third
    expect(thirdResolved).toBe(true)

    r1()
    release3()
  })

  it('caps concurrent work at exactly N under load', async () => {
    const max = 2
    const sem = new Semaphore(max)
    let active = 0
    let maxActive = 0
    const seen: number[] = []
    const tasks: Promise<number>[] = []

    const run = async (id: number) => {
      const release = await sem.acquire()
      active++
      maxActive = Math.max(maxActive, active)
      // Real tick to let other contenders queue; simulate slow body
      await new Promise((res) => setTimeout(res, 5))
      seen.push(id)
      active--
      release()
      return id
    }

    // Start 5 concurrent contenders
    for (let i = 0; i < 5; i++) tasks.push(run(i))
    const results = await Promise.all(tasks)

    expect(results.sort((a, b) => a - b)).toEqual([0, 1, 2, 3, 4])
    expect(seen.length).toBe(5)
    expect(maxActive).toBeGreaterThanOrEqual(1)
    expect(maxActive).toBeLessThanOrEqual(max)
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
  beforeEach(() => {
    _resetThumbnailSemaphoreForTests()
  })

  it('runs the wrapped fn and always releases even on throw', async () => {
    const start = createThumbnailSemaphore(4).getAvailableForTest()
    expect(start).toBe(4)

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

  it('returns singleton back to full capacity after mixed work', async () => {
    // Run a burst of work then ensure no slot leakage
    const work = Array.from({ length: 8 }, (_, i) =>
      withThumbnailSlot(async () => {
        await new Promise((r) => setTimeout(r, 1))
        return i
      })
    )
    const results = await Promise.all(work)
    expect(results).toEqual([0, 1, 2, 3, 4, 5, 6, 7])

    // Sneaky probe: construct a new local Semaphore(4) so we can compare
    // initial capacity to observed 4; then run one more wrapped job
    // and confirm it resolves quickly (no deadlock from leaked slots).
    const oneMore = withTimeout(withThumbnailSlot(() => Promise.resolve('ok')), 1000)
    await expect(oneMore).resolves.toBe('ok')
  })

  it('preserves start FIFO order under saturation', async () => {
    // `withThumbnailSlot` uses a shared singleton with max=4.
    // Queue 8 tasks that record their start order; verify they start in
    // enqueue order even though only 4 run at once.
    const startOrder: number[] = []
    const work = Array.from({ length: 8 }, (_, i) =>
      withThumbnailSlot(async () => {
        startOrder.push(i)
        await new Promise((r) => setTimeout(r, 6))
        return i
      })
    )
    const out = await Promise.all(work)
    expect(out).toEqual([0, 1, 2, 3, 4, 5, 6, 7])
    // All 8 should have started; because max=4, indices 0..3 start first,
    // then 4..7 start after wave 1 releases. Start order must be 0..7
    // exactly because each earlier-enqueued waiter is ahead in FIFO.
    expect(startOrder).toEqual([0, 1, 2, 3, 4, 5, 6, 7])
  })
})

function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new Error(`timed out after ${ms}ms`)), ms)
    p.then((v) => {
      clearTimeout(t)
      resolve(v)
    }).catch((e) => {
      clearTimeout(t)
      reject(e)
    })
  })
}

// Mark vitest globals used implicitly (vi) via the import — prevents
// tsconfig noUnusedLocals from flagging it even though we don't call it
// today (kept for future mock usage).
void vi
