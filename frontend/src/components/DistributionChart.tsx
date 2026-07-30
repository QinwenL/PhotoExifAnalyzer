import {
  PieChart,
  Pie,
  Cell,
  ResponsiveContainer,
  Tooltip,
  Legend,
} from 'recharts'

interface PieChartData {
  name: string
  value: number
  percentage: number
}

interface DistributionChartProps {
  data: PieChartData[]
  title: string
}

const COLORS = [
  'hsl(var(--primary))',
  'hsl(var(--secondary))',
  'hsl(var(--accent))',
  'hsl(var(--muted))',
  'hsl(var(--destructive))',
  '#8884d8',
  '#82ca9d',
  '#ffc658',
  '#ff7c7c',
  '#8dd1e1',
]

export function DistributionChart({ data, title }: DistributionChartProps) {
  if (!data || data.length === 0) {
    return (
      <div className="text-sm text-muted-foreground text-center py-4">
        暂无{title}数据
      </div>
    )
  }

  // Show top 10, group rest as "其他"
  const chartData = data.length > 10
    ? [
        ...data.slice(0, 9),
        {
          name: '其他',
          value: data.slice(9).reduce((sum, item) => sum + item.value, 0),
          percentage: data.slice(9).reduce((sum, item) => sum + item.percentage, 0),
        },
      ]
    : data

  return (
    <div className="w-full h-64">
      <ResponsiveContainer width="100%" height="100%">
        <PieChart>
          <Pie
            data={chartData}
            cx="50%"
            cy="50%"
            labelLine={false}
            label={({ name, percent }) => `${name} (${((percent ?? 0) * 100).toFixed(0)}%)`}
            outerRadius={80}
            fill="#8884d8"
            dataKey="value"
          >
            {chartData.map((_, index) => (
              <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
            ))}
          </Pie>
          <Tooltip
            formatter={(value) => [value, '数量']}
          />
          <Legend />
        </PieChart>
      </ResponsiveContainer>
    </div>
  )
}
