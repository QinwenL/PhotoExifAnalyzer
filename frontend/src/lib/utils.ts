import { type ClassValue, clsx } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/**
 * Extract the file name (last segment) from a path.
 *
 * Handles both POSIX (`/`) and Windows (`\`) separators, which matters
 * because Tauri on Windows can return paths with either separator
 * depending on whether they originated from a user-picked directory
 * (backslashes) or from Rust path joins (forward slashes).
 *
 * Replaces the previous `path.split(/[/\\]/).pop()` pattern that was
 * duplicated across 7 call sites (App.tsx, ImageDetail.tsx, Thumbnail.tsx,
 * ConfirmDialog.tsx).
 *
 * Returns `''` for paths ending in a separator and for the empty string,
 * matching the behavior of the original inline expression.
 */
export function getFileName(path: string): string {
  return path.split(/[/\\]/).pop() ?? ''
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

