import { beforeEach, describe, expect, it, vi } from 'vitest'

// Mock @tauri-apps/api so store.ts can be imported in a node test env
// without a real Tauri runtime. Only `sortResults` is exercised here;
// the store's async actions (which call invoke/listen) are covered by
// integration testing in the running app.
vi.mock('@tauri-apps/api/tauri', () => ({
  invoke: vi.fn(),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}))

import { sortResults, type ScanResult } from '../store'

function makeResult(
  path: string,
  overrides: Partial<ScanResult['exif']> & { fileSize?: number } = {}
): ScanResult {
  return {
    path,
    exif: {
      make: overrides.make,
      model: overrides.model,
      lens_model: overrides.lens_model,
      focal_length: overrides.focal_length,
      aperture: overrides.aperture,
      iso: overrides.iso,
      exposure_time: overrides.exposure_time,
      datetime_original: overrides.datetime_original,
    },
    file_size: overrides.fileSize ?? 0,
    error: undefined,
  }
}

describe('sortResults', () => {
  let results: ScanResult[]

  beforeEach(() => {
    results = [
      makeResult('/photos/zebra.jpg', {
        make: 'Nikon',
        model: 'Z6',
        datetime_original: '2024-03-15T10:00:00',
        fileSize: 3000,
      }),
      makeResult('/photos/apple.jpg', {
        make: 'Canon',
        model: 'EOS R5',
        datetime_original: '2024-01-01T08:00:00',
        fileSize: 1000,
      }),
      makeResult('/photos/mango.jpg', {
        make: 'Sony',
        model: 'A7',
        datetime_original: '2024-02-20T12:00:00',
        fileSize: 2000,
      }),
    ]
  })

  it('sorts by name ascending (localeCompare)', () => {
    const sorted = sortResults(results, 'name', 'asc')
    expect(sorted.map((r) => r.path)).toEqual([
      '/photos/apple.jpg',
      '/photos/mango.jpg',
      '/photos/zebra.jpg',
    ])
  })

  it('sorts by name descending', () => {
    const sorted = sortResults(results, 'name', 'desc')
    expect(sorted.map((r) => r.path)).toEqual([
      '/photos/zebra.jpg',
      '/photos/mango.jpg',
      '/photos/apple.jpg',
    ])
  })

  it('sorts by date ascending', () => {
    const sorted = sortResults(results, 'date', 'asc')
    expect(sorted.map((r) => r.exif.datetime_original)).toEqual([
      '2024-01-01T08:00:00',
      '2024-02-20T12:00:00',
      '2024-03-15T10:00:00',
    ])
  })

  it('sorts by date descending', () => {
    const sorted = sortResults(results, 'date', 'desc')
    expect(sorted.map((r) => r.exif.datetime_original)).toEqual([
      '2024-03-15T10:00:00',
      '2024-02-20T12:00:00',
      '2024-01-01T08:00:00',
    ])
  })

  it('sorts by size ascending', () => {
    const sorted = sortResults(results, 'size', 'asc')
    expect(sorted.map((r) => r.file_size)).toEqual([1000, 2000, 3000])
  })

  it('sorts by size descending', () => {
    const sorted = sortResults(results, 'size', 'desc')
    expect(sorted.map((r) => r.file_size)).toEqual([3000, 2000, 1000])
  })

  it('sorts by camera ascending (formatCamera output)', () => {
    const sorted = sortResults(results, 'camera', 'asc')
    expect(sorted.map((r) => r.exif.make)).toEqual(['Canon', 'Nikon', 'Sony'])
  })

  it('sorts by camera descending', () => {
    const sorted = sortResults(results, 'camera', 'desc')
    expect(sorted.map((r) => r.exif.make)).toEqual(['Sony', 'Nikon', 'Canon'])
  })

  it('does not mutate the original array', () => {
    const original = [...results]
    sortResults(results, 'name', 'asc')
    // Original array order should be unchanged
    expect(results.map((r) => r.path)).toEqual(original.map((r) => r.path))
  })

  it('handles missing datetime_original (treated as empty string)', () => {
    const withMissing: ScanResult[] = [
      makeResult('/photos/b.jpg', { datetime_original: '2024-01-01T00:00:00' }),
      makeResult('/photos/a.jpg', { datetime_original: undefined }),
    ]
    const sorted = sortResults(withMissing, 'date', 'asc')
    // '' < '2024-...' so missing date comes first
    expect(sorted.map((r) => r.path)).toEqual(['/photos/a.jpg', '/photos/b.jpg'])
  })

  it('handles missing camera (treated as empty string)', () => {
    const withMissing: ScanResult[] = [
      makeResult('/photos/b.jpg', { make: 'Canon', model: 'EOS R5' }),
      makeResult('/photos/a.jpg', {}),
    ]
    const sorted = sortResults(withMissing, 'camera', 'asc')
    // '' < 'Canon EOS R5' so missing camera comes first
    expect(sorted.map((r) => r.path)).toEqual(['/photos/a.jpg', '/photos/b.jpg'])
  })

  it('returns empty array for empty input', () => {
    expect(sortResults([], 'name', 'asc')).toEqual([])
  })

  it('preserves single-element input', () => {
    const single = [makeResult('/photos/only.jpg')]
    expect(sortResults(single, 'name', 'asc')).toHaveLength(1)
  })
})
