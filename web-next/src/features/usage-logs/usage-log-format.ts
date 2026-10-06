export function formatUsageNumber(value: number, locale: string) {
  return new Intl.NumberFormat(locale).format(value)
}

export function formatUsageLatency(value: number | null, locale: string) {
  if (value === null) return '-'
  if (value < 1_000) return `${formatUsageNumber(value, locale)} ms`
  return `${(value / 1_000).toLocaleString(locale, {
    minimumFractionDigits: 1,
    maximumFractionDigits: value < 10_000 ? 2 : 1,
  })} s`
}

export type UsageLatencyKind = 'first-token' | 'duration'
export type UsageLatencyTone = 'neutral' | 'success' | 'warning' | 'destructive'

/** 首字与总耗时使用独立阈值，颜色只辅助数字本身，不替代可读标签。 */
export function usageLatencyTone(value: number | null, kind: UsageLatencyKind): UsageLatencyTone {
  if (value === null) return 'neutral'
  const warningThreshold = kind === 'first-token' ? 1_000 : 5_000
  const destructiveThreshold = kind === 'first-token' ? 5_000 : 15_000
  if (value <= warningThreshold) return 'success'
  if (value <= destructiveThreshold) return 'warning'
  return 'destructive'
}

export function formatCompactRequestId(value: string) {
  return value.length <= 18 ? value : `${value.slice(0, 9)}...${value.slice(-6)}`
}

export function formatUsageDate(value: number, locale: string) {
  return new Intl.DateTimeFormat(locale, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(value * 1_000)
}

export function formatUsageDateLong(value: number, locale: string) {
  return new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'medium',
  }).format(value * 1_000)
}
