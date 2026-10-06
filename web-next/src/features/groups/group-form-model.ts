import { z } from 'zod'

import type { AdminGroup, AdminGroupWriteRequest } from '@/lib/api/generated/types.gen'

export type GroupEditorMode = 'create' | 'update'

export type GroupFormValues = {
  name: string
  displayName: string
  ratio: string
  peakEnabled: boolean
  peakRatio: string
  peakStart: string
  peakEnd: string
  isExclusive: boolean
  dailyLimit: string
  weeklyLimit: string
  monthlyLimit: string
  rpmLimit: string
  fallbackGroupId: string
  claudeCodeOnly: boolean
}

export type GroupValidationMessages = {
  invalidName: string
  invalidDisplayName: string
  invalidRatio: string
  invalidNumber: string
  invalidGroup: string
  invalidPeakWindow: string
}

const MICROS_PER_UNIT = 1_000_000n
const MAX_SAFE_INTEGER = BigInt(Number.MAX_SAFE_INTEGER)
const MAX_I32 = 2_147_483_647n
const RATIO_PATTERN = /^(0|[1-9][0-9]{0,9})(?:\.([0-9]{1,6}))?$/
const INTEGER_PATTERN = /^(0|[1-9][0-9]*)$/
const TIME_PATTERN = /^(?:[01][0-9]|2[0-3]):[0-5][0-9](?::[0-5][0-9])?$/

/** 按百万分固定精度解析倍率，整个换算过程不经过浮点运算。 */
export function ratioToMicros(value: string): number | undefined {
  const match = RATIO_PATTERN.exec(value)
  if (!match) return undefined
  const whole = BigInt(match[1])
  const fraction = BigInt((match[2] ?? '').padEnd(6, '0') || '0')
  const micros = whole * MICROS_PER_UNIT + fraction
  return micros <= MAX_SAFE_INTEGER ? Number(micros) : undefined
}

/** 将服务端百万分整数还原为最短十进制倍率文本。 */
export function ratioFromMicros(value: number): string {
  if (!Number.isSafeInteger(value) || value < 0) return ''
  const micros = BigInt(value)
  const whole = micros / MICROS_PER_UNIT
  const fraction = String(micros % MICROS_PER_UNIT).padStart(6, '0').replace(/0+$/, '')
  return fraction ? `${whole}.${fraction}` : String(whole)
}

/** 前端复现公开分组写入边界，自引用回退和高峰三字段在同一校验器内闭合。 */
export function buildGroupFormSchema(
  currentGroupId: number | undefined,
  messages: GroupValidationMessages,
) {
  return z.object({
    name: z.string().refine((value) => validText(value, 64), messages.invalidName),
    displayName: z.string().refine((value) => validText(value, 128), messages.invalidDisplayName),
    ratio: z.string().refine((value) => ratioToMicros(value) !== undefined, messages.invalidRatio),
    peakEnabled: z.boolean(),
    peakRatio: z.string(),
    peakStart: z.string(),
    peakEnd: z.string(),
    isExclusive: z.boolean(),
    dailyLimit: optionalIntegerSchema(MAX_SAFE_INTEGER, messages.invalidNumber),
    weeklyLimit: optionalIntegerSchema(MAX_SAFE_INTEGER, messages.invalidNumber),
    monthlyLimit: optionalIntegerSchema(MAX_SAFE_INTEGER, messages.invalidNumber),
    rpmLimit: optionalIntegerSchema(MAX_I32, messages.invalidNumber),
    fallbackGroupId: optionalIntegerSchema(MAX_SAFE_INTEGER, messages.invalidGroup, 1n),
    claudeCodeOnly: z.boolean(),
  }).superRefine((values, context) => {
    if (values.peakEnabled) {
      const validStart = TIME_PATTERN.test(values.peakStart)
      const validEnd = TIME_PATTERN.test(values.peakEnd)
      if (ratioToMicros(values.peakRatio) === undefined) {
        context.addIssue({ code: 'custom', message: messages.invalidRatio, path: ['peakRatio'] })
      }
      if (!validStart) {
        context.addIssue({ code: 'custom', message: messages.invalidPeakWindow, path: ['peakStart'] })
      }
      if (!validEnd || (validStart && normalizeTime(values.peakStart) === normalizeTime(values.peakEnd))) {
        context.addIssue({ code: 'custom', message: messages.invalidPeakWindow, path: ['peakEnd'] })
      }
    }
    const fallbackGroupId = parseOptionalInteger(values.fallbackGroupId, MAX_SAFE_INTEGER, 1n)
    if (currentGroupId !== undefined && fallbackGroupId === currentGroupId) {
      context.addIssue({ code: 'custom', message: messages.invalidGroup, path: ['fallbackGroupId'] })
    }
  })
}

export function defaultGroupFormValues(group?: AdminGroup): GroupFormValues {
  const peakEnabled = group?.peak_ratio_micros !== null && group?.peak_ratio_micros !== undefined
  return {
    name: group?.name ?? '',
    displayName: group?.display_name ?? '',
    ratio: group ? ratioFromMicros(group.ratio_micros) : '1',
    peakEnabled,
    peakRatio: peakEnabled ? ratioFromMicros(group.peak_ratio_micros ?? 0) : '',
    peakStart: peakEnabled ? group?.peak_start ?? '' : '',
    peakEnd: peakEnabled ? group?.peak_end ?? '' : '',
    isExclusive: group?.is_exclusive ?? false,
    dailyLimit: formatOptionalInteger(group?.daily_limit),
    weeklyLimit: formatOptionalInteger(group?.weekly_limit),
    monthlyLimit: formatOptionalInteger(group?.monthly_limit),
    rpmLimit: formatOptionalInteger(group?.rpm_limit),
    fallbackGroupId: formatOptionalInteger(group?.fallback_group_id),
    claudeCodeOnly: group?.flags.claude_code_only === true,
  }
}

/** 生成完整写模型；未知 flags 原样透传，仅覆盖本页拥有的结构化开关。 */
export function toGroupRequest(
  values: GroupFormValues,
  existing?: AdminGroup,
): AdminGroupWriteRequest {
  const peakRatioMicros = values.peakEnabled ? requireRatio(values.peakRatio) : null
  return {
    name: values.name,
    display_name: values.displayName,
    ratio_micros: requireRatio(values.ratio),
    peak_ratio_micros: peakRatioMicros,
    peak_start: values.peakEnabled ? normalizeTime(values.peakStart) : null,
    peak_end: values.peakEnabled ? normalizeTime(values.peakEnd) : null,
    is_exclusive: values.isExclusive,
    daily_limit: requireOptionalInteger(values.dailyLimit, MAX_SAFE_INTEGER),
    weekly_limit: requireOptionalInteger(values.weeklyLimit, MAX_SAFE_INTEGER),
    monthly_limit: requireOptionalInteger(values.monthlyLimit, MAX_SAFE_INTEGER),
    rpm_limit: requireOptionalInteger(values.rpmLimit, MAX_I32),
    fallback_group_id: requireOptionalInteger(values.fallbackGroupId, MAX_SAFE_INTEGER, 1n),
    flags: {
      ...(existing?.flags ?? {}),
      claude_code_only: values.claudeCodeOnly,
    },
  }
}

function optionalIntegerSchema(maximum: bigint, message: string, minimum = 0n) {
  return z.string().refine(
    (value) => parseOptionalInteger(value, maximum, minimum) !== undefined,
    message,
  )
}

function parseOptionalInteger(
  value: string,
  maximum: bigint,
  minimum = 0n,
): number | null | undefined {
  if (!value) return null
  if (!INTEGER_PATTERN.test(value)) return undefined
  const parsed = BigInt(value)
  return parsed >= minimum && parsed <= maximum ? Number(parsed) : undefined
}

function requireOptionalInteger(value: string, maximum: bigint, minimum = 0n) {
  const parsed = parseOptionalInteger(value, maximum, minimum)
  if (parsed === undefined) throw new Error('分组表单可选整数未通过校验')
  return parsed
}

function requireRatio(value: string) {
  const ratio = ratioToMicros(value)
  if (ratio === undefined) throw new Error('分组倍率未通过校验')
  return ratio
}

function normalizeTime(value: string) {
  if (!TIME_PATTERN.test(value)) throw new Error('高峰时间未通过校验')
  return value.length === 5 ? `${value}:00` : value
}

function validText(value: string, maximumBytes: number) {
  return value.length > 0
    && value.trim() === value
    && new TextEncoder().encode(value).length <= maximumBytes
    && ![...value].some((character) => /\p{Cc}/u.test(character))
}

function formatOptionalInteger(value?: number | null) {
  return value === null || value === undefined ? '' : String(value)
}
