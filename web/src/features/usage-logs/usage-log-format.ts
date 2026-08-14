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
