export type QuotaDisplayMode = 'quota' | 'custom_unit'
export type QuotaDisplaySymbolPosition = 'prefix' | 'suffix'

export type QuotaDisplayPolicy = {
  mode: QuotaDisplayMode
  unit_name: string
  unit_symbol: string
  quota_units_per_display_unit: string
  symbol_position: QuotaDisplaySymbolPosition
  fraction_digits: number
}

export const DEFAULT_QUOTA_DISPLAY_POLICY: QuotaDisplayPolicy = {
  mode: 'quota',
  unit_name: '算力积分',
  unit_symbol: '积分',
  quota_units_per_display_unit: '10000',
  symbol_position: 'suffix',
  fraction_digits: 0,
}

/** 使用整数运算格式化余额展示，策略不会反向参与计费。 */
export function formatQuota(
  quota: bigint | number | string,
  policy: QuotaDisplayPolicy,
  locale?: string,
): string {
  const quotaValue = parseInteger(quota)
  if (policy.mode === 'quota') {
    return formatGroupedInteger(quotaValue, locale)
  }

  const denominator = parsePositiveInteger(policy.quota_units_per_display_unit)
  const fractionDigits = normalizeFractionDigits(policy.fraction_digits)
  const negative = quotaValue < 0n
  const absolute = negative ? -quotaValue : quotaValue
  const scale = 10n ** BigInt(fractionDigits)
  const numerator = absolute * scale
  let rounded = numerator / denominator
  const remainder = numerator % denominator
  if (remainder * 2n >= denominator) rounded += 1n

  const integer = rounded / scale
  const fraction = rounded % scale
  const sign = negative && rounded !== 0n ? '-' : ''
  const number = fractionDigits === 0
    ? formatGroupedInteger(integer, locale)
    : `${formatGroupedInteger(integer, locale)}.${fraction.toString().padStart(fractionDigits, '0')}`

  return policy.symbol_position === 'prefix'
    ? `${sign}${policy.unit_symbol}${number}`
    : `${sign}${number}\u00a0${policy.unit_symbol}`
}

/** 返回完整原始额度，供展示策略预览和审计对照使用。 */
export function formatRawQuota(quota: bigint | number | string, locale?: string): string {
  return formatGroupedInteger(parseInteger(quota), locale)
}

/** 检查两个策略是否会重新缩放所有历史余额展示。 */
export function balanceDisplayScaleChanged(
  before: QuotaDisplayPolicy,
  after: QuotaDisplayPolicy,
): boolean {
  return before.mode !== after.mode
    || before.quota_units_per_display_unit !== after.quota_units_per_display_unit
}

function parseInteger(value: bigint | number | string): bigint {
  if (typeof value === 'bigint') return value
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value)) throw new RangeError('quota 必须是安全整数')
    return BigInt(value)
  }
  if (!/^-?(?:0|[1-9][0-9]*)$/.test(value)) throw new RangeError('quota 必须是十进制整数')
  return BigInt(value)
}

function parsePositiveInteger(value: string): bigint {
  if (!/^[1-9][0-9]*$/.test(value)) throw new RangeError('展示面额必须是正整数')
  return BigInt(value)
}

function normalizeFractionDigits(value: number): number {
  if (!Number.isInteger(value) || value < 0 || value > 4) {
    throw new RangeError('展示小数位必须位于 0 到 4')
  }
  return value
}

function formatGroupedInteger(value: bigint, locale?: string): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(value)
}
