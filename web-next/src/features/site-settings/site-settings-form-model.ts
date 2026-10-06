import { z } from 'zod'

import type {
  AdminSiteSettings,
  AdminSiteSettingsRequest,
} from '@/lib/api/generated/types.gen'
import {
  DEFAULT_QUOTA_DISPLAY_POLICY,
  type QuotaDisplayPolicy,
} from '../../lib/quota-display.ts'

export type SiteSettingsValues = {
  siteName: string
  publicBaseUrl: string
  logoUrl: string
  tagline: string
  description: string
  balanceMode: 'quota' | 'custom_unit'
  unitName: string
  unitSymbol: string
  quotaUnitsPerDisplayUnit: string
  symbolPosition: 'prefix' | 'suffix'
  fractionDigits: number
}

type SiteSettingsValidationMessages = {
  siteName: string
  publicBaseUrl: string
  logoUrl: string
  tagline: string
  description: string
  unitName: string
  unitSymbol: string
  quotaUnitsPerDisplayUnit: string
  fractionDigits: string
}

/** 从服务端快照构造表单值，仅在滚动升级缺字段时使用展示默认值。 */
export function siteSettingsValues(settings: AdminSiteSettings): SiteSettingsValues {
  const balanceDisplay = siteSettingsBalanceDisplay(settings)
  return {
    siteName: settings.site_name,
    publicBaseUrl: settings.public_base_url ?? '',
    logoUrl: settings.brand.logo_url ?? '',
    tagline: settings.brand.tagline ?? '',
    description: settings.brand.description ?? '',
    balanceMode: balanceDisplay.mode,
    unitName: balanceDisplay.unit_name,
    unitSymbol: balanceDisplay.unit_symbol,
    quotaUnitsPerDisplayUnit: balanceDisplay.quota_units_per_display_unit,
    symbolPosition: balanceDisplay.symbol_position,
    fractionDigits: balanceDisplay.fraction_digits,
  }
}

/** 兼容滚动升级期间缺少余额展示字段的旧响应，避免站点设置整页崩溃。 */
export function siteSettingsBalanceDisplay(settings: AdminSiteSettings): QuotaDisplayPolicy {
  return settings.balance_display ?? DEFAULT_QUOTA_DISPLAY_POLICY
}

/** 复现站点设置的文本和 URL 边界，最终结果仍以后端校验为准。 */
export function buildSiteSettingsSchema(messages: SiteSettingsValidationMessages) {
  return z.object({
    siteName: z.string().refine(
      (value) => validRequiredText(value, 80, false),
      messages.siteName,
    ),
    publicBaseUrl: z.string().refine(validOptionalPublicBaseUrl, messages.publicBaseUrl),
    logoUrl: z.string().refine(validOptionalLogoUrl, messages.logoUrl),
    tagline: z.string().refine(
      (value) => validOptionalText(value, 160, false),
      messages.tagline,
    ),
    description: z.string().refine(
      (value) => validOptionalText(value, 500, true),
      messages.description,
    ),
    balanceMode: z.enum(['quota', 'custom_unit']),
    unitName: z.string().refine(
      (value) => validRequiredText(value, 64, false),
      messages.unitName,
    ),
    unitSymbol: z.string().refine(
      (value) => validRequiredText(value, 24, false),
      messages.unitSymbol,
    ),
    quotaUnitsPerDisplayUnit: z.string().refine(validPositiveI64, messages.quotaUnitsPerDisplayUnit),
    symbolPosition: z.enum(['prefix', 'suffix']),
    fractionDigits: z.number().int().min(0, messages.fractionDigits).max(4, messages.fractionDigits),
  })
}

/** 把空白可选字段归一化为 null，保持请求语义稳定。 */
export function toSiteSettingsRequest(
  values: SiteSettingsValues,
  expectedVersion: number,
): AdminSiteSettingsRequest {
  return {
    site_name: values.siteName.trim(),
    public_base_url: optionalValue(values.publicBaseUrl),
    brand: {
      logo_url: optionalValue(values.logoUrl),
      tagline: optionalValue(values.tagline),
      description: optionalValue(values.description),
    },
    balance_display: balanceDisplayPolicy(values),
    expected_version: expectedVersion,
  }
}

/** 从已校验表单生成只用于展示的策略快照。 */
export function balanceDisplayPolicy(values: SiteSettingsValues): QuotaDisplayPolicy {
  return {
    mode: values.balanceMode,
    unit_name: values.unitName.trim(),
    unit_symbol: values.unitSymbol.trim(),
    quota_units_per_display_unit: values.quotaUnitsPerDisplayUnit,
    symbol_position: values.symbolPosition,
    fraction_digits: values.fractionDigits,
  }
}

/** 仅让已通过 Logo URL 边界的草稿进入预览。 */
export function previewLogoUrl(value: string) {
  const normalized = value.trim()
  return normalized && validOptionalLogoUrl(normalized) ? normalized : undefined
}

function optionalValue(value: string) {
  const normalized = value.trim()
  return normalized || null
}

function validRequiredText(value: string, maximumBytes: number, multiline: boolean) {
  return value.length > 0
    && value.trim() === value
    && byteLength(value) <= maximumBytes
    && !hasForbiddenControl(value, multiline)
}

function validOptionalText(value: string, maximumBytes: number, multiline: boolean) {
  return value.length === 0 || validRequiredText(value, maximumBytes, multiline)
}

function validOptionalPublicBaseUrl(value: string) {
  if (value.length === 0) return true
  if (value.trim() !== value || byteLength(value) > 2_048 || hasForbiddenControl(value, false)) {
    return false
  }
  try {
    const url = new URL(value)
    return (url.protocol === 'http:' || url.protocol === 'https:')
      && Boolean(url.hostname)
      && !url.username
      && !url.password
      && !url.search
      && !url.hash
  } catch {
    return false
  }
}

function validOptionalLogoUrl(value: string) {
  if (value.length === 0) return true
  if (
    value.trim() !== value
    || byteLength(value) > 2_048
    || value.includes('\\')
    || hasForbiddenControl(value, false)
  ) {
    return false
  }
  if (value.startsWith('/')) return !value.startsWith('//')
  try {
    const url = new URL(value)
    return (url.protocol === 'http:' || url.protocol === 'https:')
      && Boolean(url.hostname)
      && !url.username
      && !url.password
      && !url.hash
  } catch {
    return false
  }
}

function validPositiveI64(value: string) {
  if (!/^[1-9][0-9]{0,18}$/.test(value)) return false
  try {
    return BigInt(value) <= 9_223_372_036_854_775_807n
  } catch {
    return false
  }
}

function byteLength(value: string) {
  return new TextEncoder().encode(value).length
}

function hasForbiddenControl(value: string, multiline: boolean) {
  return [...value].some((character) => {
    if (multiline && (character === '\n' || character === '\r' || character === '\t')) return false
    return /\p{Cc}/u.test(character)
  })
}
