import { type ClassValue, clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/**
 * Format the camera display name from make and model.
 *
 * - Both present → `"Canon EOS R5"`
 * - Only make    → `"Canon"`
 * - Only model   → `"iPhone 15 Pro"`
 * - Neither      → `undefined`
 *
 * Single source of truth mirroring `ExifData::camera_name()` in the Rust
 * backend (`src-tauri/src/exif/mod.rs`). Previously duplicated in
 * `App.tsx`, `store.ts`, and `ImageDetail.tsx`.
 */
export function formatCamera(exif: {
  make?: string
  model?: string
}): string | undefined {
  if (exif.make && exif.model) {
    return `${exif.make} ${exif.model}`
  }
  return exif.make || exif.model
}

