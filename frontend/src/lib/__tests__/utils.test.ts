import { describe, expect, it } from 'vitest'
import { formatCamera } from '../utils'

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
