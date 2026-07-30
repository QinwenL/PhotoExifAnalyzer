import { useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import type { ScanResult } from '../store'

interface VirtualizedGridProps {
  items: ScanResult[]
  columns: number
  renderItem: (item: ScanResult, index: number) => React.ReactNode
}

export function VirtualizedGrid({ items, columns, renderItem }: VirtualizedGridProps) {
  const parentRef = useRef<HTMLDivElement>(null)

  const rowCount = Math.ceil(items.length / columns)

  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual 返回的函数无法被 React Compiler 安全 memoize，此处允许跳过编译
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 200, // Estimated row height
    overscan: 5,
  })

  return (
    <div ref={parentRef} className="h-full overflow-auto">
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const startIndex = virtualRow.index * columns
          const rowItems = items.slice(startIndex, startIndex + columns)

          return (
            <div
              key={virtualRow.key}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              <div className="grid gap-2 h-full" style={{ gridTemplateColumns: `repeat(${columns}, 1fr)` }}>
                {rowItems.map((item, colIndex) => renderItem(item, startIndex + colIndex))}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

interface VirtualizedListProps {
  items: ScanResult[]
  renderItem: (item: ScanResult, index: number) => React.ReactNode
}

export function VirtualizedList({ items, renderItem }: VirtualizedListProps) {
  const parentRef = useRef<HTMLDivElement>(null)

  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 48, // Estimated row height
    overscan: 10,
  })

  return (
    <div ref={parentRef} className="h-full overflow-auto">
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => (
          <div
            key={virtualRow.key}
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              width: '100%',
              height: `${virtualRow.size}px`,
              transform: `translateY(${virtualRow.start}px)`,
            }}
          >
            {renderItem(items[virtualRow.index], virtualRow.index)}
          </div>
        ))}
      </div>
    </div>
  )
}
