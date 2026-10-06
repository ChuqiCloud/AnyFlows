import { useId, useState, type CSSProperties } from 'react'

type ChartPoint = { time: number; value: number }
type DashboardChartProps = {
  points: ChartPoint[]
  label: string
  color: string
  formatTime: (value: number) => string
  formatValue: (value: number) => string
}

/** Keep empty hours and zero values in their actual positions. */
export function DashboardChart({ points, label, color, formatTime, formatValue }: DashboardChartProps) {
  const id = useId().replaceAll(':', '')
  const [active, setActive] = useState<number | null>(null)
  const maximum = Math.max(1, ...points.map(point => point.value))
  const magnitude = 10 ** Math.floor(Math.log10(maximum / 4))
  const step = Math.max(1, ([1, 2, 5, 10].find(value => value * magnitude >= maximum / 4) ?? 10) * magnitude)
  const ceiling = Math.ceil(maximum / step) * step
  const ticks = Array.from({ length: Math.round(ceiling / step) + 1 }, (_, index) => ceiling - index * step)
  const x = (index: number) => 8 + index / Math.max(1, points.length - 1) * 784
  const y = (value: number) => 180 - value / ceiling * 160
  const coordinates = points.map((point, index) => `${x(index)},${y(point.value)}`)
  const line = coordinates.length ? `M ${coordinates.join(' L ')}` : ''
  const area = coordinates.length ? `${line} L ${x(points.length - 1)},180 L 8,180 Z` : ''
  const selectedIndex = active !== null && active < points.length ? active : Math.max(0, points.length - 1)
  const selected = points[selectedIndex]

  return (
    <div className="report-chart" style={{ '--report-series': color } as CSSProperties}>
      <div className="report-chart-readout" aria-live="polite">
        <span>{selected ? formatTime(selected.time) : '--'}</span>
        <span><i aria-hidden="true" />{label} <strong>{selected ? formatValue(selected.value) : '--'}</strong></span>
      </div>
      <div className="report-chart-plot">
        <div className="report-chart-scale" aria-hidden="true">
          {ticks.map(value => <span key={value}>{formatValue(value)}</span>)}
        </div>
        <div className="report-chart-canvas">
          <svg viewBox="0 0 800 200" preserveAspectRatio="none" role="img" aria-label={label}>
            <defs><linearGradient id={id} x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor={color} stopOpacity="0.24" /><stop offset="100%" stopColor={color} stopOpacity="0.015" /></linearGradient></defs>
            {ticks.map(value => <line key={value} x1="0" x2="800" y1={y(value)} y2={y(value)} stroke="var(--hairline)" strokeDasharray="3 5" />)}
            <path d={area} fill={`url(#${id})`} />
            <path d={line} fill="none" stroke={color} strokeWidth="2.5" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
            {selected && <>
              <line x1={x(selectedIndex)} x2={x(selectedIndex)} y1="15" y2="180" stroke={color} strokeOpacity="0.35" strokeDasharray="4 4" />
              <circle cx={x(selectedIndex)} cy={y(selected.value)} r="4" fill="var(--card)" stroke={color} strokeWidth="2" />
            </>}
          </svg>
          <div className="report-chart-hitareas">
            {points.map((point, index) => <button key={point.time} type="button"
              tabIndex={index === selectedIndex ? 0 : -1}
              aria-label={`${formatTime(point.time)} ${label} ${formatValue(point.value)}`}
              onPointerEnter={() => setActive(index)} onClick={() => setActive(index)} onFocus={() => setActive(index)}
              onKeyDown={event => {
                if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
                  event.preventDefault()
                  const next = Math.min(points.length - 1, Math.max(0, index + (event.key === 'ArrowRight' ? 1 : -1)))
                  setActive(next)
                  const sibling = event.currentTarget.parentElement?.children[next] as HTMLButtonElement | undefined
                  sibling?.focus()
                }
              }} />)}
          </div>
        </div>
      </div>
      <div className="report-chart-times" aria-hidden="true">
        {[0, Math.floor(points.length / 3), Math.floor(points.length * 2 / 3), points.length - 1].map((index, position) => <span key={position}>{points[index] ? formatTime(points[index].time) : '--'}</span>)}
      </div>
    </div>
  )
}
