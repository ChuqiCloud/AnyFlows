import { z } from 'zod'

import type {
  AdminSiteSettings,
  AdminSiteSettingsRequest,
} from '@/lib/api/generated/types.gen'

export type SiteSettingsValues = {
  siteName: string
  publicBaseUrl: string
  logoUrl: string
  tagline: string
  description: string
}

type SiteSettingsValidationMessages = {
  siteName: string
  publicBaseUrl: string
  logoUrl: string
  tagline: string
  description: string
}

/** 从服务端完整快照构造表单值，不在前端补写业务默认值。 */
export function siteSettingsValues(settings: AdminSiteSettings): SiteSettingsValues {
  return {
    siteName: settings.site_name,
    publicBaseUrl: settings.public_base_url ?? '',
    logoUrl: settings.brand.logo_url ?? '',
    tagline: settings.brand.tagline ?? '',
    description: settings.brand.description ?? '',
  }
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
  })
}

/** 把空白可选字段归一化为 null，保持请求语义稳定。 */
export function toSiteSettingsRequest(values: SiteSettingsValues): AdminSiteSettingsRequest {
  return {
    site_name: values.siteName.trim(),
    public_base_url: optionalValue(values.publicBaseUrl),
    brand: {
      logo_url: optionalValue(values.logoUrl),
      tagline: optionalValue(values.tagline),
      description: optionalValue(values.description),
    },
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

function byteLength(value: string) {
  return new TextEncoder().encode(value).length
}

function hasForbiddenControl(value: string, multiline: boolean) {
  return [...value].some((character) => {
    if (multiline && (character === '\n' || character === '\r' || character === '\t')) return false
    return /\p{Cc}/u.test(character)
  })
}
