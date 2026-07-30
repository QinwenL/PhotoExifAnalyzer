/**
 * 解释后端 `delete_images_with_progress` 返回的 per-path 结果。
 *
 * 后端 Rust 签名：`Vec<Result<(), String>>`
 * serde 默认序列化为外部标签枚举：
 *   - `Ok(())`  → `{ Ok: null }`
 *   - `Err(s)`  → `{ Err: "msg" }`
 *
 * 为兼容非标准序列化器或未来变化，额外容忍以下形态：
 *   - `null` / `undefined`           → 视为成功
 *   - `{ ok: ... }` / `{ Ok: ... }`  → 视为成功
 *   - `{ err: "msg" }` / `{ Err: "msg" }` / `{ error: "msg" }` → 视为失败
 *   - 字符串                          → 视为失败（错误消息）
 */

export type BackendDeleteResult = unknown

export interface DeleteOutcome {
  /** 成功删除的路径索引列表 */
  succeededIndices: number[]
  /** 失败删除的路径索引到错误消息的映射 */
  failedIndices: Map<number, string>
}

/**
 * 给定后端返回的 per-path 结果数组和对应路径，返回成功/失败的索引分类。
 *
 * 注意：`results[i]` 为 null/undefined 时视为成功（serde 将 `Ok(())`
 * 序列化为 null）；但 `i >= results.length`（结果缺失）视为失败，
 * 避免在后端返回不完整时误删列表项。
 */
export function interpretDeleteResults(
  results: BackendDeleteResult[],
  paths: string[]
): DeleteOutcome {
  const succeededIndices: number[] = []
  const failedIndices = new Map<number, string>()

  for (let i = 0; i < paths.length; i++) {
    // 结果数组比 paths 短：后端返回不完整，保守视为失败
    if (i >= results.length) {
      failedIndices.set(i, `missing result for path: ${paths[i]}`)
      continue
    }

    const r = results[i]
    const err = extractErrorMessage(r)
    if (err === null) {
      succeededIndices.push(i)
    } else {
      failedIndices.set(i, err)
    }
  }

  return { succeededIndices, failedIndices }
}

function extractErrorMessage(value: unknown): string | null {
  // null/undefined → 成功
  if (value === null || value === undefined) {
    return null
  }

  // 字符串 → 错误消息（untagged Err）
  if (typeof value === 'string') {
    return value
  }

  if (typeof value === 'object') {
    const v = value as Record<string, unknown>
    // 标准 serde 外部标签：{ Ok: ... } / { Err: "msg" }
    if ('Ok' in v) return null
    if ('ok' in v) return null
    if ('Err' in v) return String(v.Err)
    if ('err' in v) return String(v.err)
    if ('error' in v) return String(v.error)
  }

  // 无法识别 → 视为失败，给出描述性消息
  return `Unrecognized result: ${JSON.stringify(value)}`
}
