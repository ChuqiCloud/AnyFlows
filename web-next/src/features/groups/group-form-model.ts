import { z } from 'zod'

import type { AdminGroup, AdminGroupWriteRequest } from '@/lib/api/generated/types.gen'
import { groupPeakWindows, MAX_PEAK_RULES, peakRulesOverlap, ruleModels, secondTime, timeSecond, type PeakRuleForm } from './group-peak-model.ts'

export type GroupEditorMode = 'create' | 'update'

export type GroupFormValues = {
  name: string
  displayName: string
  ratio: string
  peakEnabled: boolean
  peakRules: PeakRuleForm[]
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
  invalidWeekdays: string
  invalidModels: string
  overlappingRules: string
  invalidRules: string
  invalidTimezone: string
}

const MICROS_PER_UNIT = 1_000_000n
const MAX_SAFE_INTEGER = BigInt(Number.MAX_SAFE_INTEGER)
const MAX_I32 = 2_147_483_647n
const RATIO_PATTERN = /^(0|[1-9][0-9]{0,9})(?:\.([0-9]{1,6}))?$/
const INTEGER_PATTERN = /^(0|[1-9][0-9]*)$/

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

/** 前端复现公开分组写入边界，统一校验回退目标、周计划和模型范围。 */
export function buildGroupFormSchema(
  currentGroupId: number | undefined,
  messages: GroupValidationMessages,
) {
  return z.object({
    name: z.string().refine((value) => validText(value, 64), messages.invalidName),
    displayName: z.string().refine((value) => validText(value, 128), messages.invalidDisplayName),
    ratio: z.string().refine((value) => ratioToMicros(value) !== undefined, messages.invalidRatio),
    peakEnabled: z.boolean(),
    peakRules: z.array(z.object({
      weekdays: z.array(z.number()),
      start: z.string(),
      end: z.string(),
      ratio: z.string(),
      modelScope: z.enum(['all', 'selected']),
      models: z.string(),
      utcOffset: z.string(),
    })),
    isExclusive: z.boolean(),
    dailyLimit: optionalIntegerSchema(MAX_SAFE_INTEGER, messages.invalidNumber),
    weeklyLimit: optionalIntegerSchema(MAX_SAFE_INTEGER, messages.invalidNumber),
    monthlyLimit: optionalIntegerSchema(MAX_SAFE_INTEGER, messages.invalidNumber),
    rpmLimit: optionalIntegerSchema(MAX_I32, messages.invalidNumber),
    fallbackGroupId: optionalIntegerSchema(MAX_SAFE_INTEGER, messages.invalidGroup, 1n),
    claudeCodeOnly: z.boolean(),
  }).superRefine((values, context) => {
    if (values.peakEnabled) {
      if (values.peakRules.length === 0 || values.peakRules.length > MAX_PEAK_RULES) {
        context.addIssue({ code: 'custom', message: messages.invalidRules, path: ['peakRules', 'root'] })
      }
      values.peakRules.forEach((rule, index) => {
        const issue = (field: keyof PeakRuleForm, message: string) =>
          context.addIssue({ code: 'custom', message, path: ['peakRules', index, field] })
        const start = timeSecond(rule.start)
        const end = timeSecond(rule.end, true)
        if (ratioToMicros(rule.ratio) === undefined) issue('ratio', messages.invalidRatio)
        if (start === undefined) issue('start', messages.invalidPeakWindow)
        if (end === undefined || start === end) issue('end', messages.invalidPeakWindow)
        if (rule.weekdays.length === 0 || rule.weekdays.length > 7
          || new Set(rule.weekdays).size !== rule.weekdays.length
          || rule.weekdays.some((day) => !Number.isInteger(day) || day < 1 || day > 7)) issue('weekdays', messages.invalidWeekdays)
        if (!/^-?(0|[1-9][0-9]*)$/.test(rule.utcOffset)
          || Number(rule.utcOffset) < -720 || Number(rule.utcOffset) > 840) issue('utcOffset', messages.invalidTimezone)
        const models = ruleModels(rule.models)
        if (rule.modelScope === 'selected' && (models.length === 0 || models.length > 64
          || new Set(models).size !== models.length
          || models.some((model) => !validText(model, 256)))) issue('models', messages.invalidModels)
        if (values.peakRules.slice(0, index).some((previous) => peakRulesOverlap(previous, rule))) issue('end', messages.overlappingRules)
      })
    }
    const fallbackGroupId = parseOptionalInteger(values.fallbackGroupId, MAX_SAFE_INTEGER, 1n)
    if (currentGroupId !== undefined && fallbackGroupId === currentGroupId) {
      context.addIssue({ code: 'custom', message: messages.invalidGroup, path: ['fallbackGroupId'] })
    }
  })
}

export function defaultGroupFormValues(group?: AdminGroup): GroupFormValues {
  const windows = groupPeakWindows(group)
  return {
    name: group?.name ?? '',
    displayName: group?.display_name ?? '',
    ratio: group ? ratioFromMicros(group.ratio_micros) : '1',
    peakEnabled: windows.length > 0,
    peakRules: windows.map((window) => ({
      weekdays: window.weekdays, start: secondTime(window.start_second), end: secondTime(window.end_second),
      ratio: ratioFromMicros(window.ratio_micros), modelScope: window.models?.length ? 'selected' : 'all',
      models: (window.models ?? []).join('\n'), utcOffset: String(window.utc_offset_minutes ?? 0),
    })),
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
  return {
    name: values.name,
    display_name: values.displayName,
    ratio_micros: requireRatio(values.ratio),
    peak_ratio_micros: null,
    peak_start: null,
    peak_end: null,
    peak_windows: values.peakEnabled ? values.peakRules.map((rule) => ({
      weekdays: rule.weekdays,
      start_second: timeSecond(rule.start)!,
      end_second: timeSecond(rule.end, true)!,
      ratio_micros: requireRatio(rule.ratio),
      models: rule.modelScope === 'all' ? [] : ruleModels(rule.models),
      utc_offset_minutes: Number(rule.utcOffset),
    })) : [],
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

function validText(value: string, maximumBytes: number) {
  return value.length > 0
    && value.trim() === value
    && new TextEncoder().encode(value).length <= maximumBytes
    && ![...value].some((character) => /\p{Cc}/u.test(character))
}

function formatOptionalInteger(value?: number | null) {
  return value === null || value === undefined ? '' : String(value)
}
