/**
 * 上次文件夹自动加载决策。
 *
 * 规格 desktop-ui/spec.md：下次打开应用 SHALL 自动加载上次的文件夹。
 *
 * 将决策逻辑提取为纯函数，便于单元测试，并让 App.tsx 挂载时的副作用
 * 仅做"读 + 判断 + 触发"，不嵌入业务逻辑。
 */

/**
 * 判断是否应在应用启动时自动加载上次扫描的目录。
 *
 * 仅检查 lastDirectory 是否有效（非 null、非空）。
 * 重复加载的避免由 useEffect 的空依赖数组保证（只在挂载时触发一次）。
 *
 * 实现为 type guard，让调用方在分支内获得 `string` 类型而非
 * `string | null`，避免向 `scanDirectoryWithProgress(dir: string)`
 * 传入可能为 null 的值。
 */
export function shouldAutoLoadLastDirectory(
  lastDirectory: string | null
): lastDirectory is string {
  return lastDirectory !== null && lastDirectory.length > 0
}

/**
 * 从类 localStorage 的存储中安全读取上次目录。
 *
 * 任何异常（如浏览器隐私模式下的 SecurityError）都被吞掉并返回 null，
 * 避免阻断应用启动。
 */
export function readLastDirectory(
  storage: { getItem: (key: string) => string | null } = localStorage
): string | null {
  try {
    const dir = storage.getItem('lastDirectory')
    return dir && dir.length > 0 ? dir : null
  } catch {
    return null
  }
}
