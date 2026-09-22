import { z } from 'zod'

import type {
  OrganizationBudgetPolicyCreateRequest,
  OrganizationBudgetPeriodRequest,
  OrganizationBudgetPolicyResponse,
} from '@/lib/api/generated/types.gen'

export type BudgetScopeKind = 'organization' | 'team' | 'member'
export type BudgetPeriodKind = 'utc_day' | 'utc_week' | 'utc_month' | 'fixed'
export type BudgetFixedUnit = 'seconds' | 'minutes' | 'hours' | 'days'

export type OrganizationBudgetFormValues = {
  scopeKind: BudgetScopeKind
  teamId: string
  membershipId: string
  periodKind: BudgetPeriodKind
  fixedValue: string
  fixedUnit: BudgetFixedUnit
  unlimited: boolean
  quotaLimit: string
  effectiveFrom: string
}

export type OrganizationBudgetValidationMessages = {
  scope: string
  fixedPeriod: string
  quotaLimit: string
  effectiveFrom: string
  effectiveConflict: string
}

type OrganizationBudgetValidationContext = {
  activeTeamIds: ReadonlySet<number>
  activeMembershipIds: ReadonlySet<number>
  policies: readonly OrganizationBudgetPolicyResponse[]
}

const INTEGER_PATTERN = /^[1-9]\d*$/
const MAX_SAFE_INTEGER = BigInt(Number.MAX_SAFE_INTEGER)
const MIN_FIXED_SECONDS = 60n
const MAX_FIXED_SECONDS = 366n * 24n * 60n * 60n
const FIXED_UNIT_SECONDS = {
  seconds: 1n,
  minutes: 60n,
  hours: 60n * 60n,
  days: 24n * 60n * 60n,
} as const satisfies Record<BudgetFixedUnit, bigint>

/** 创建表单使用当前分钟之后的第一个完整分钟，减少时间边界冲突。 */
export function defaultOrganizationBudgetValues(now = new Date()): OrganizationBudgetFormValues {
  const nextMinute = Math.ceil((now.getTime() + 1) / 60_000) * 60_000
  return {
    scopeKind: 'organization',
    teamId: '',
    membershipId: '',
    periodKind: 'utc_month',
    fixedValue: '30',
    fixedUnit: 'days',
    unlimited: false,
    quotaLimit: '',
    effectiveFrom: unixSecondsToLocalInput(Math.floor(nextMinute / 1_000)),
  }
}

/** 校验主体归属、整数额度、自定义周期和不可回退的策略生效时间。 */
export function buildOrganizationBudgetSchema(
  context: OrganizationBudgetValidationContext,
  messages: OrganizationBudgetValidationMessages,
) {
  return z.object({
    scopeKind: z.enum(['organization', 'team', 'member']),
    teamId: z.string(),
    membershipId: z.string(),
    periodKind: z.enum(['utc_day', 'utc_week', 'utc_month', 'fixed']),
    fixedValue: z.string(),
    fixedUnit: z.enum(['seconds', 'minutes', 'hours', 'days']),
    unlimited: z.boolean(),
    quotaLimit: z.string(),
    effectiveFrom: z.string(),
  }).superRefine((values, refinement) => {
    const selectedId = selectedScopeId(values)
    if (
      (values.scopeKind === 'team' && (selectedId === undefined || !context.activeTeamIds.has(selectedId)))
      || (values.scopeKind === 'member' && (selectedId === undefined || !context.activeMembershipIds.has(selectedId)))
    ) {
      refinement.addIssue({ code: 'custom', message: messages.scope, path: [scopeField(values.scopeKind)] })
    }
    if (values.periodKind === 'fixed' && fixedPeriodSeconds(values.fixedValue, values.fixedUnit) === undefined) {
      refinement.addIssue({ code: 'custom', message: messages.fixedPeriod, path: ['fixedValue'] })
    }
    if (!values.unlimited && positiveSafeInteger(values.quotaLimit) === undefined) {
      refinement.addIssue({ code: 'custom', message: messages.quotaLimit, path: ['quotaLimit'] })
    }
    const effectiveFrom = localInputToUnixSeconds(values.effectiveFrom)
    if (effectiveFrom === undefined) {
      refinement.addIssue({ code: 'custom', message: messages.effectiveFrom, path: ['effectiveFrom'] })
      return
    }
    const latest = latestScopeEffectiveFrom(context.policies, values)
    if (latest !== undefined && effectiveFrom <= latest) {
      refinement.addIssue({ code: 'custom', message: messages.effectiveConflict, path: ['effectiveFrom'] })
    }
  })
}

/** 将已校验表单转换为闭合 API 请求，不使用浮点额度或周期换算。 */
export function toOrganizationBudgetRequest(
  values: OrganizationBudgetFormValues,
): OrganizationBudgetPolicyCreateRequest {
  const effectiveFrom = localInputToUnixSeconds(values.effectiveFrom)
  const quotaLimit = values.unlimited ? null : positiveSafeInteger(values.quotaLimit)
  if (effectiveFrom === undefined || quotaLimit === undefined) {
    throw new Error('企业预算表单尚未通过校验')
  }
  const scope = values.scopeKind === 'organization'
    ? { kind: 'organization' as const }
    : values.scopeKind === 'team'
      ? { kind: 'team' as const, team_id: requireScopeId(values.teamId) }
      : { kind: 'member' as const, membership_id: requireScopeId(values.membershipId) }
  const period = budgetPeriodRequest(values)
  return {
    scope,
    period,
    quota_limit: quotaLimit,
    effective_from: effectiveFrom,
  }
}

/** 将用户选择的整数与单位精确换算为服务端固定周期秒数。 */
export function fixedPeriodSeconds(value: string, unit: BudgetFixedUnit) {
  if (!INTEGER_PATTERN.test(value)) return undefined
  const seconds = BigInt(value) * FIXED_UNIT_SECONDS[unit]
  return seconds >= MIN_FIXED_SECONDS && seconds <= MAX_FIXED_SECONDS
    ? Number(seconds)
    : undefined
}

/** 返回同一作用域历史中最新的生效时间，用于表单提示与冲突预防。 */
export function latestScopeEffectiveFrom(
  policies: readonly OrganizationBudgetPolicyResponse[],
  values: Pick<OrganizationBudgetFormValues, 'scopeKind' | 'teamId' | 'membershipId'>,
) {
  const selectedId = selectedScopeId(values)
  const matches = policies.filter((policy) => {
    if (policy.scope.kind !== values.scopeKind) return false
    if (policy.scope.kind === 'team') return policy.scope.team_id === selectedId
    if (policy.scope.kind === 'member') return policy.scope.membership_id === selectedId
    return true
  })
  return matches.reduce<number | undefined>(
    (latest, policy) => latest === undefined || policy.effective_from > latest
      ? policy.effective_from
      : latest,
    undefined,
  )
}

/** 用稳定文本键关联策略、窗口和真实目录标签。 */
export function organizationBudgetScopeKey(
  scope: OrganizationBudgetPolicyResponse['scope'],
) {
  if (scope.kind === 'team') return `team:${scope.team_id}`
  if (scope.kind === 'member') return `member:${scope.membership_id}`
  return 'organization'
}

function selectedScopeId(
  values: Pick<OrganizationBudgetFormValues, 'scopeKind' | 'teamId' | 'membershipId'>,
) {
  if (values.scopeKind === 'organization') return undefined
  return positiveSafeInteger(values.scopeKind === 'team' ? values.teamId : values.membershipId)
}

function scopeField(scope: BudgetScopeKind) {
  return scope === 'team' ? 'teamId' : 'membershipId'
}

function positiveSafeInteger(value: string) {
  if (!INTEGER_PATTERN.test(value)) return undefined
  const parsed = BigInt(value)
  return parsed <= MAX_SAFE_INTEGER ? Number(parsed) : undefined
}

function localInputToUnixSeconds(value: string) {
  if (!value) return undefined
  const milliseconds = new Date(value).getTime()
  if (!Number.isFinite(milliseconds) || milliseconds < 0) return undefined
  const seconds = Math.floor(milliseconds / 1_000)
  return Number.isSafeInteger(seconds) ? seconds : undefined
}

function unixSecondsToLocalInput(seconds: number) {
  const date = new Date(seconds * 1_000)
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000)
  return local.toISOString().slice(0, 16)
}

function requireScopeId(value: string) {
  const parsed = positiveSafeInteger(value)
  if (parsed === undefined) throw new Error('企业预算作用域未通过校验')
  return parsed
}

function requireFixedSeconds(value: string, unit: BudgetFixedUnit) {
  const seconds = fixedPeriodSeconds(value, unit)
  if (seconds === undefined) throw new Error('企业预算固定周期未通过校验')
  return seconds
}

function budgetPeriodRequest(values: OrganizationBudgetFormValues): OrganizationBudgetPeriodRequest {
  switch (values.periodKind) {
    case 'utc_day': return { kind: 'utc_day' }
    case 'utc_week': return { kind: 'utc_week' }
    case 'utc_month': return { kind: 'utc_month' }
    case 'fixed': return {
      kind: 'fixed',
      seconds: requireFixedSeconds(values.fixedValue, values.fixedUnit),
    }
  }
}
