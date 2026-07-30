import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts'
import type { FocalLengthStats } from '../store'

interface FocalLengthChartProps {
  stats: FocalLengthStats | null
}

export function FocalLengthChart({ stats }: FocalLengthChartProps) {
  if (!stats || stats.ranges.length === 0) {
    return (
      <div className="text-sm text-muted-foreground text-center py-4">
        暂无焦距数据
      </div>
    )
  }

  const data = stats.ranges.map((range) => ({
    name: range.label,
    count: range.count,
    percentage: range.percentage.toFixed(1),
  }))

  return (
    <div className="w-full h-64">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={data} margin={{ top: 5, right: 30, left: 20, bottom: 5 }}>
          <CartesianGrid strokeDasharray="3 3" className="opacity-30" />
          <XAxis
            dataKey="name"
            tick={{ fontSize: 10 }}
            angle={-45}
            textAnchor="end"
            height={60}
          />
          <YAxis tick={{ fontSize: 12 }} />
          <Tooltip
            formatter={(value) => [value, '数量']}
            labelFormatter={(label) => `焦距范围: ${label}`}
          />
          <Bar dataKey="count" fill="hsl(var(--primary))" radius={[4, 4, 0, 0]} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  )
}
