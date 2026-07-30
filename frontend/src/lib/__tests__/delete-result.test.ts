import { describe, expect, it } from 'vitest'
import { interpretDeleteResults } from '../delete-result'

describe('interpretDeleteResults', () => {
  const paths = ['/a.jpg', '/b.jpg', '/c.jpg']

  it('标准 serde 格式：{Ok: null} 成功，{Err: "msg"} 失败', () => {
    const results = [
      { Ok: null },
      { Err: 'Permission denied' },
      { Ok: null },
    ]
    const outcome = interpretDeleteResults(results, paths)
    expect(outcome.succeededIndices).toEqual([0, 2])
    expect(outcome.failedIndices.get(1)).toBe('Permission denied')
  })

  it('全部成功', () => {
    const results = [{ Ok: null }, { Ok: null }, { Ok: null }]
    const outcome = interpretDeleteResults(results, paths)
    expect(outcome.succeededIndices).toEqual([0, 1, 2])
    expect(outcome.failedIndices.size).toBe(0)
  })

  it('全部失败', () => {
    const results = [
      { Err: 'err1' },
      { Err: 'err2' },
      { Err: 'err3' },
    ]
    const outcome = interpretDeleteResults(results, paths)
    expect(outcome.succeededIndices).toEqual([])
    expect(outcome.failedIndices.size).toBe(3)
  })

  it('null/undefined 视为成功（兼容性）', () => {
    const results = [null, undefined, { Ok: null }]
    const outcome = interpretDeleteResults(results, paths)
    expect(outcome.succeededIndices).toEqual([0, 1, 2])
  })

  it('字符串视为错误消息（untagged Err 兼容）', () => {
    const results = ['File not found', { Ok: null }]
    const outcome = interpretDeleteResults(results, [paths[0], paths[1]])
    expect(outcome.failedIndices.get(0)).toBe('File not found')
    expect(outcome.succeededIndices).toEqual([1])
  })

  it('小写 key 兼容：{ok: ...} / {err: ...}', () => {
    const results = [{ ok: true }, { err: 'failed' }]
    const outcome = interpretDeleteResults(results, [paths[0], paths[1]])
    expect(outcome.succeededIndices).toEqual([0])
    expect(outcome.failedIndices.get(1)).toBe('failed')
  })

  it('{error: "msg"} 形态兼容', () => {
    const results = [{ error: 'disk full' }]
    const outcome = interpretDeleteResults(results, [paths[0]])
    expect(outcome.failedIndices.get(0)).toBe('disk full')
  })

  it('无法识别的形态视为失败并给出描述', () => {
    const results = [42]
    const outcome = interpretDeleteResults(results, [paths[0]])
    expect(outcome.failedIndices.size).toBe(1)
    expect(outcome.failedIndices.get(0)).toContain('Unrecognized')
  })

  it('空结果数组返回空 outcome', () => {
    const outcome = interpretDeleteResults([], [])
    expect(outcome.succeededIndices).toEqual([])
    expect(outcome.failedIndices.size).toBe(0)
  })

  it('结果数组比 paths 短时缺失索引视为失败（保守保留）', () => {
    const results = [{ Ok: null }] // 只有 1 个结果，但 paths 有 3 个
    const outcome = interpretDeleteResults(results, paths)
    expect(outcome.succeededIndices).toEqual([0])
    // 索引 1 和 2 没有对应结果 → 视为失败，避免误删
    expect(outcome.failedIndices.size).toBe(2)
    expect(outcome.failedIndices.get(1)).toContain('missing')
    expect(outcome.failedIndices.get(2)).toContain('missing')
  })
})
