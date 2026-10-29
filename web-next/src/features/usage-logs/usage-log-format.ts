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

/** 首字只看自身延迟：<5s 绿、5~10s 黄、>10s 红。 */
export function firstTokenTone(value: number | null): UsageLatencyTone {
  if (value === null) return 'neutral'
  if (value < 5_000) return 'success'
  return value < 10_000 ? 'warning' : 'destructive'
}

/** 耗时按吞吐量判断，输出不足 100 token 时退回纯耗时阈值。 */
export function durationTone(duration: number | null, outputTokens: number): UsageLatencyTone {
  if (duration === null) return 'neutral'
  const seconds = duration / 1_000
  if (outputTokens < 100 || seconds <= 0) {
    if (seconds < 10) return 'success'
    return seconds < 30 ? 'warning' : 'destructive'
  }
  const throughput = outputTokens / seconds
  if (throughput >= 30) return 'success'
  return throughput >= 15 ? 'warning' : 'destructive'
}

/** 兼容旧调用：不带输出量时 duration 等价于只看耗时。 */
export function usageLatencyTone(value: number | null, kind: UsageLatencyKind, outputTokens = 0): UsageLatencyTone {
  return kind === 'first-token' ? firstTokenTone(value) : durationTone(value, outputTokens)
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
