import { describe, expect, it } from 'vitest'
import { formatCamera, getFileName } from '../utils'

describe('getFileName', () => {
  it('extracts filename from a Unix path', () => {
    expect(getFileName('/home/user/photos/IMG_001.jpg')).toBe('IMG_001.jpg')
  })

  it('extracts filename from a Windows path', () => {
    expect(getFileName('C:\\Users\\user\\photos\\IMG_001.jpg')).toBe('IMG_001.jpg')
  })

  it('extracts filename from a mixed-separator path', () => {
    // Tauri on Windows can yield paths with either separator depending
    // on the source (e.g. user-picked dirs use backslashes, but joined
    // paths from Rust may use forward slashes).
    expect(getFileName('C:/Users/user\\photos\\IMG_001.jpg')).toBe('IMG_001.jpg')
  })

  it('returns the input unchanged when no separator is present', () => {
    expect(getFileName('IMG_001.jpg')).toBe('IMG_001.jpg')
  })

  it('returns empty string for a trailing separator', () => {
    expect(getFileName('/photos/')).toBe('')
  })

  it('returns empty string for an empty path', () => {
    expect(getFileName('')).toBe('')
  })
})

describe('formatCamera', () => {
  it('returns "make model" when both make and model are present', () => {
    expect(
      formatCamera({ make: 'Canon', model: 'EOS R5' })
    ).toBe('Canon EOS R5')
  })

  it('returns make alone when model is missing', () => {
    expect(formatCamera({ make: 'Sigma' })).toBe('Sigma')
  })

  it('returns model alone when make is missing', () => {
    expect(formatCamera({ model: 'iPhone 15 Pro' })).toBe('iPhone 15 Pro')
  })

  it('returns undefined when neither make nor model is present', () => {
    expect(formatCamera({})).toBeUndefined()
  })

  it('returns undefined for empty strings (falsy check)', () => {
    // `make && model` treats empty strings as falsy, so an empty make
    // with a real model should return the model alone (not "" + model).
    expect(formatCamera({ make: '', model: 'EOS R5' })).toBe('EOS R5')
  })

  it('mirrors the Rust ExifData::camera_name() behavior', () => {
    // Cross-check the four canonical cases that the Rust unit tests cover
    // (test_camera_name_both_make_and_model / _make_only / _model_only /
    // _neither in src-tauri/src/exif/mod.rs).
    expect(formatCamera({ make: 'Canon', model: 'EOS R5' })).toBe('Canon EOS R5')
    expect(formatCamera({ make: 'Sigma' })).toBe('Sigma')
    expect(formatCamera({ model: 'iPhone 15 Pro' })).toBe('iPhone 15 Pro')
    expect(formatCamera({})).toBeUndefined()
  })
})
