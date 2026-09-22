import { Chip } from '@heroui/react'
import { CalendarClock, History, TimerReset } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import type { TFunction } from 'i18next'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import type {
  OrganizationBudgetOverviewResponse,
  OrganizationBudgetPeriodResponse,
  OrganizationBudgetPolicyResponse,
  OrganizationBudgetScopeResponse,
  OrganizationBudgetWindowResponse,
  OrganizationMembershipResponse,
  OrganizationTeamResponse,
} from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { organizationBudgetScopeKey } from './organization-budget-form-model'

type OrganizationBudgetOverviewProps = {
  overview: OrganizationBudgetOverviewResponse
  teams: OrganizationTeamResponse[]
  members: OrganizationMembershipResponse[]
  formatQuota: (value: number) => string
  formatTime: (value: number) => string
}

/** 展示当前窗口和不可变策略历史，窄屏使用条目布局避免横向表格溢出。 */
export function OrganizationBudgetOverview({
  overview,
  teams,
  members,
  formatQuota,
  formatTime,
}: OrganizationBudgetOverviewProps) {
  const { t } = useTranslation()
  const teamMap = useMemo(() => new Map(teams.map((team) => [team.id, team])), [teams])
  const memberMap = useMemo(() => new Map(members.map((member) => [member.id, member])), [members])
  const policies = [...overview.policies].sort((left, right) => (
    right.effective_from - left.effective_from || right.version - left.version
  ))
  const policyById = new Map(overview.policies.map((policy) => [policy.id, policy]))
  const windowPolicyIds = new Set(overview.windows.map((window) => window.policy_id))
  const now = Math.floor(Date.now() / 1_000)
  const currentEntries: Array<{
    policy: OrganizationBudgetPolicyResponse
    window?: OrganizationBudgetWindowResponse
  }> = overview.windows.flatMap((window) => {
    const policy = policyById.get(window.policy_id)
    return policy ? [{ policy, window }] : []
  })
  currentEntries.push(...overview.policies
    .filter((policy) => !windowPolicyIds.has(policy.id) && (
      policy.effective_from > now
      || (policy.effective_from <= now && (policy.effective_until === null || policy.effective_until > now))
    ))
    .map((policy) => ({ policy })))
  currentEntries.sort((left, right) => (
    organizationBudgetScopeKey(left.policy.scope).localeCompare(organizationBudgetScopeKey(right.policy.scope))
    || left.policy.effective_from - right.policy.effective_from
  ))
  const scopeLabel = (scope: OrganizationBudgetScopeResponse) => {
    if (scope.kind === 'team') {
      const team = teamMap.get(scope.team_id)
      return team?.name ?? t('organizationBudgets.scope.unknownTeam', { id: scope.team_id })
    }
    if (scope.kind === 'member') {
      const member = memberMap.get(scope.membership_id)
      return member
        ? t('organizationBudgets.scope.memberLabel', { membershipId: member.id, userId: member.user_id })
        : t('organizationBudgets.scope.unknownMember', { id: scope.membership_id })
    }
    return t('organizationBudgets.scope.organizationLabel')
  }

  return (
    <div className="grid gap-6">
      <section aria-labelledby="current-budget-heading">
        <div className="mb-3 flex flex-wrap items-end justify-between gap-2">
          <div>
            <h3 id="current-budget-heading" className="text-sm font-semibold">
              {t('organizationBudgets.current.title')}
            </h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('organizationBudgets.current.subtitle')}
            </p>
          </div>
          <Chip className="bg-surface-2 text-muted-foreground" size="sm" variant="flat">
            {t('organizationBudgets.current.count', { count: currentEntries.length })}
          </Chip>
        </div>

        {currentEntries.length === 0 ? (
          <EmptySection
            icon={TimerReset}
            title={t('organizationBudgets.current.emptyTitle')}
            body={t('organizationBudgets.current.emptyBody')}
          />
        ) : (
          <div className="grid gap-3">
            {currentEntries.map(({ policy, window }) => {
              const future = policy.effective_from * 1_000 > Date.now()
              return (
                <article key={window ? `window:${window.id}` : `policy:${policy.id}`} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1/35 p-4">
                  <div className="flex min-w-0 flex-col gap-2 sm:flex-row sm:items-start sm:justify-between">
                    <div className="min-w-0">
                      <div className="flex min-w-0 flex-wrap items-center gap-2">
                        <h4 className="truncate text-sm font-semibold">{scopeLabel(policy.scope)}</h4>
                        <Chip size="sm" variant="flat">{t('organizationBudgets.history.version', { version: policy.version })}</Chip>
                        <Chip className={cn(
                          window ? 'text-success' : future ? 'text-info' : 'text-muted-foreground',
                        )} size="sm" variant="flat">
                          {t(window
                            ? 'organizationBudgets.current.active'
                            : future
                              ? 'organizationBudgets.current.scheduled'
                              : policy.quota_limit === null
                                ? 'organizationBudgets.current.unlimited'
                                : 'organizationBudgets.current.awaiting')}
                        </Chip>
                      </div>
                      <p className="mt-1 text-[0.6875rem] text-muted-foreground">
                        {periodLabel(policy.period, t)} · {t('organizationBudgets.current.effective', { value: formatTime(policy.effective_from) })}
                      </p>
                    </div>
                    <span className="shrink-0 text-xs font-medium tabular-nums">
                      {policy.quota_limit === null
                        ? t('organizationBudgets.values.unlimited')
                        : formatQuota(policy.quota_limit)}
                    </span>
                  </div>

                  {window ? (
                    <>
                      <dl className="mt-4 grid grid-cols-2 gap-x-4 gap-y-3 border-t border-[var(--hairline)] pt-4 sm:grid-cols-4">
                        <BudgetMetric label={t('organizationBudgets.metrics.limit')} value={formatQuota(window.limit_snapshot)} />
                        <BudgetMetric label={t('organizationBudgets.metrics.used')} value={formatQuota(window.used_quota)} />
                        <BudgetMetric label={t('organizationBudgets.metrics.frozen')} value={formatQuota(window.frozen_quota)} />
                        <BudgetMetric label={t('organizationBudgets.metrics.remaining')} value={formatQuota(window.remaining_quota)} tone="text-success" />
                      </dl>
                      <p className="mt-4 flex items-center gap-1.5 text-[0.6875rem] text-muted-foreground">
                        <CalendarClock className="size-3.5 shrink-0" aria-hidden="true" />
                        <span>{t('organizationBudgets.current.windowRange', {
                          start: formatTime(window.window_starts_at),
                          end: formatTime(window.window_ends_at),
                        })}</span>
                      </p>
                    </>
                  ) : (
                    <p className="mt-4 border-t border-[var(--hairline)] pt-3 text-xs leading-5 text-muted-foreground">
                      {t(future
                        ? 'organizationBudgets.current.scheduledBody'
                        : policy.quota_limit === null
                          ? 'organizationBudgets.current.unlimitedBody'
                          : 'organizationBudgets.current.awaitingBody')}
                    </p>
                  )}
                </article>
              )
            })}
          </div>
        )}
      </section>

      <section aria-labelledby="budget-history-heading">
        <div className="mb-3">
          <h3 id="budget-history-heading" className="text-sm font-semibold">
            {t('organizationBudgets.history.title')}
          </h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t('organizationBudgets.history.subtitle')}
          </p>
        </div>
        {policies.length === 0 ? (
          <EmptySection
            icon={History}
            title={t('organizationBudgets.history.emptyTitle')}
            body={t('organizationBudgets.history.emptyBody')}
          />
        ) : (
          <div className="overflow-hidden rounded-lg border border-[var(--hairline)]">
            {policies.map((policy, index) => (
              <PolicyHistoryRow
                key={policy.id}
                policy={policy}
                scope={scopeLabel(policy.scope)}
                formatQuota={formatQuota}
                formatTime={formatTime}
                separated={index > 0}
              />
            ))}
          </div>
        )}
      </section>
    </div>
  )
}

function BudgetMetric({ label, value, tone }: { label: string; value: string; tone?: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-[0.6875rem] text-muted-foreground">{label}</dt>
      <dd className={cn('mt-1 truncate text-sm font-semibold tabular-nums', tone)} title={value}>{value}</dd>
    </div>
  )
}

function PolicyHistoryRow({
  policy,
  scope,
  formatQuota,
  formatTime,
  separated,
}: {
  policy: OrganizationBudgetPolicyResponse
  scope: string
  formatQuota: (value: number) => string
  formatTime: (value: number) => string
  separated: boolean
}) {
  const { t } = useTranslation()
  return (
    <article className={cn(
      'grid min-w-0 gap-3 bg-surface-1/25 px-4 py-3.5 sm:grid-cols-[minmax(0,1.25fr)_minmax(0,1fr)_minmax(0,1fr)] sm:items-center',
      separated && 'border-t border-[var(--hairline)]',
    )}>
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-1.5">
          <span className="truncate text-xs font-medium">{scope}</span>
          <Chip size="sm" variant="flat">{t('organizationBudgets.history.version', { version: policy.version })}</Chip>
          {policy.effective_until === null ? <Chip className="text-success" size="sm" variant="flat">{t('organizationBudgets.history.current')}</Chip> : null}
        </div>
        <p className="mt-1 text-[0.6875rem] text-muted-foreground">{periodLabel(policy.period, t)}</p>
      </div>
      <div className="min-w-0">
        <p className="text-[0.6875rem] text-muted-foreground">{t('organizationBudgets.history.limit')}</p>
        <p className="mt-1 truncate text-xs font-semibold tabular-nums" title={policy.quota_limit === null ? undefined : formatQuota(policy.quota_limit)}>
          {policy.quota_limit === null ? t('organizationBudgets.values.unlimited') : formatQuota(policy.quota_limit)}
        </p>
      </div>
      <div className="min-w-0 text-[0.6875rem] leading-4 text-muted-foreground sm:text-right">
        <p>{t('organizationBudgets.history.starts', { value: formatTime(policy.effective_from) })}</p>
        <p className="mt-1">{policy.effective_until === null
          ? t('organizationBudgets.history.noEnd')
          : t('organizationBudgets.history.ends', { value: formatTime(policy.effective_until) })}</p>
      </div>
    </article>
  )
}

function EmptySection({
  icon: Icon,
  title,
  body,
}: {
  icon: LucideIcon
  title: string
  body: string
}) {
  return (
    <div className="grid min-h-40 place-items-center rounded-lg border border-dashed border-[var(--hairline)] p-5 text-center">
      <div className="max-w-sm">
        <Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" />
        <h4 className="mt-3 text-sm font-semibold">{title}</h4>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>
      </div>
    </div>
  )
}

function periodLabel(period: OrganizationBudgetPeriodResponse, t: TFunction) {
  return period.kind === 'fixed'
    ? t('organizationBudgets.period.fixedSeconds', { seconds: period.seconds })
    : t(`organizationBudgets.period.${period.kind}`)
}
