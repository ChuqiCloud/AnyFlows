const CONTEXT_UNITS = [
  { divisor: 1_000_000, suffix: 'M' },
  { divisor: 1_000, suffix: 'K' },
] as const

/** 将上下文 Token 数压缩为模型行业常用的 K/M 单位，完整值由调用方保留在提示中。 */
export function formatContextWindow(value: number): string {
  const unit = CONTEXT_UNITS.find(({ divisor }) => value >= divisor)
  if (!unit) return String(value)

  const precision = unit.suffix === 'M' ? 10 : 1
  const rounded = Math.round((value / unit.divisor) * precision) / precision
  if (unit.suffix === 'K' && rounded >= 1_000) {
    return `${Math.round((value / 1_000_000) * 10) / 10}M`
  }
  return `${rounded}${unit.suffix}`
}

/** 按当前界面语言格式化完整 Token 数，用于无损悬停提示。 */
export function formatExactContextWindow(value: number, locale?: string): string {
  return new Intl.NumberFormat(locale).format(value)
}
