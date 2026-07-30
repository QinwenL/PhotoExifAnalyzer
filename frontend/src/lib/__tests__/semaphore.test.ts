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

    await Promise.resolve()
    expect(thirdResolved).toBe(false)

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
