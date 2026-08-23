import { zodResolver } from '@hookform/resolvers/zod'
import { AlertTriangle, LoaderCircle, RefreshCw } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Switch } from '@/components/ui/switch'
import type {
  OrganizationBudgetPolicyResponse,
  OrganizationMembershipResponse,
  OrganizationTeamResponse,
} from '@/lib/api/generated/types.gen'
import {
  organizationBudgetErrorCode,
  useCreateOrganizationBudgetPolicy,
} from './organization-budget-api'
import {
  buildOrganizationBudgetSchema,
  defaultOrganizationBudgetValues,
  latestScopeEffectiveFrom,
  toOrganizationBudgetRequest,
  type OrganizationBudgetFormValues,
} from './organization-budget-form-model'

type OrganizationBudgetPolicySheetProps = {
  organizationId: number
  open: boolean
  onOpenChange: (open: boolean) => void
  policies: OrganizationBudgetPolicyResponse[]
  teams: OrganizationTeamResponse[]
  members: OrganizationMembershipResponse[]
  directoriesPending: boolean
  directoriesError: boolean
  onRefresh: () => Promise<unknown>
  onCreated: (policy: OrganizationBudgetPolicyResponse) => void
}

/** 创建企业预算的下一不可变版本，并保留冲突与结果未知的恢复路径。 */
export function OrganizationBudgetPolicySheet({
  organizationId,
  open,
  onOpenChange,
  policies,
  teams,
  members,
  directoriesPending,
  directoriesError,
  onRefresh,
  onCreated,
}: OrganizationBudgetPolicySheetProps) {
  const { i18n, t } = useTranslation()
  const activeTeams = teams.filter((team) => team.status === 'active')
  const activeMembers = members.filter((member) => member.status === 'active')
  const mutation = useCreateOrganizationBudgetPolicy(organizationId)
  const resetMutation = mutation.reset
  const [verifying, setVerifying] = useState(false)
  const schema = buildOrganizationBudgetSchema({
    activeTeamIds: new Set(activeTeams.map((team) => team.id)),
    activeMembershipIds: new Set(activeMembers.map((member) => member.id)),
    policies,
  }, {
    scope: t('organizationBudgets.validation.scope'),
    fixedPeriod: t('organizationBudgets.validation.fixedPeriod'),
    quotaLimit: t('organizationBudgets.validation.quotaLimit'),
    effectiveFrom: t('organizationBudgets.validation.effectiveFrom'),
    effectiveConflict: t('organizationBudgets.validation.effectiveConflict'),
  })
  const form = useForm<OrganizationBudgetFormValues>({
    defaultValues: defaultOrganizationBudgetValues(),
    resolver: zodResolver(schema),
  })
  const scopeKind = form.watch('scopeKind')
  const teamId = form.watch('teamId')
  const membershipId = form.watch('membershipId')
  const periodKind = form.watch('periodKind')
  const unlimited = form.watch('unlimited')
  const latestEffectiveFrom = latestScopeEffectiveFrom(policies, { scopeKind, teamId, membershipId })
  const errorCode = mutation.error ? organizationBudgetErrorCode(mutation.error) ?? 'unknown' : undefined
  const outcomeUnknown = errorCode === 'budget_outcome_unknown'
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'

  useEffect(() => {
    if (open) {
      form.reset(defaultOrganizationBudgetValues())
      resetMutation()
    }
  }, [form, open, organizationId, resetMutation])

  const submit = form.handleSubmit(async (values) => {
    try {
      const policy = await mutation.mutateAsync(toOrganizationBudgetRequest(values))
      onCreated(policy)
      onOpenChange(false)
    } catch {
      // 失败时保留用户输入；结果未知必须先查询历史，不能盲目创建重复版本。
    }
  })

  const refreshAfterError = async () => {
    setVerifying(true)
    try {
      await onRefresh()
      resetMutation()
    } finally {
      setVerifying(false)
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full gap-0 sm:max-w-xl">
        <SheetHeader className="border-b border-[var(--hairline)]">
          <SheetTitle>{t('organizationBudgets.form.title')}</SheetTitle>
          <SheetDescription>{t('organizationBudgets.form.subtitle')}</SheetDescription>
        </SheetHeader>

        <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit} noValidate>
          <div className="flex-1 space-y-5 overflow-y-auto p-4">
            <section className="grid gap-3" aria-labelledby="budget-scope-fields">
              <h3 id="budget-scope-fields" className="text-xs font-semibold">
                {t('organizationBudgets.form.scopeSection')}
              </h3>
              <FormField
                id="budget-scope-kind"
                label={t('organizationBudgets.form.scopeKind')}
              >
                <Select id="budget-scope-kind" {...form.register('scopeKind')}>
                  <option value="organization">{t('organizationBudgets.scope.organization')}</option>
                  <option value="team">{t('organizationBudgets.scope.team')}</option>
                  <option value="member">{t('organizationBudgets.scope.member')}</option>
                </Select>
              </FormField>

              {scopeKind === 'team' ? (
                <FormField
                  id="budget-team-id"
                  label={t('organizationBudgets.form.team')}
                  error={form.formState.errors.teamId?.message}
                  hint={directoriesError ? t('organizationBudgets.form.directoryError') : undefined}
                >
                  <Select
                    id="budget-team-id"
                    disabled={directoriesPending || directoriesError}
                    aria-invalid={!!form.formState.errors.teamId}
                    {...form.register('teamId')}
                  >
                    <option value="">{t(directoriesPending ? 'organizationBudgets.form.directoryLoading' : 'organizationBudgets.form.selectTeam')}</option>
                    {activeTeams.map((team) => <option key={team.id} value={team.id}>{team.name}</option>)}
                  </Select>
                </FormField>
              ) : null}

              {scopeKind === 'member' ? (
                <FormField
                  id="budget-membership-id"
                  label={t('organizationBudgets.form.member')}
                  error={form.formState.errors.membershipId?.message}
                  hint={directoriesError ? t('organizationBudgets.form.directoryError') : undefined}
                >
                  <Select
                    id="budget-membership-id"
                    disabled={directoriesPending || directoriesError}
                    aria-invalid={!!form.formState.errors.membershipId}
                    {...form.register('membershipId')}
                  >
                    <option value="">{t(directoriesPending ? 'organizationBudgets.form.directoryLoading' : 'organizationBudgets.form.selectMember')}</option>
                    {activeMembers.map((member) => (
                      <option key={member.id} value={member.id}>
                        {t('organizationBudgets.memberOption', {
                          membershipId: member.id,
                          userId: member.user_id,
                          role: t(`organizationBudgets.roles.${member.role}`, { defaultValue: member.role }),
                        })}
                      </option>
                    ))}
                  </Select>
                </FormField>
              ) : null}
            </section>

            <section className="grid gap-3" aria-labelledby="budget-period-fields">
              <h3 id="budget-period-fields" className="text-xs font-semibold">
                {t('organizationBudgets.form.periodSection')}
              </h3>
              <FormField id="budget-period-kind" label={t('organizationBudgets.form.periodKind')}>
                <Select id="budget-period-kind" {...form.register('periodKind')}>
                  <option value="utc_day">{t('organizationBudgets.period.utc_day')}</option>
                  <option value="utc_week">{t('organizationBudgets.period.utc_week')}</option>
                  <option value="utc_month">{t('organizationBudgets.period.utc_month')}</option>
                  <option value="fixed">{t('organizationBudgets.period.fixed')}</option>
                </Select>
              </FormField>
              {periodKind === 'fixed' ? (
                <div className="grid grid-cols-[minmax(0,1fr)_8.5rem] gap-2">
                  <FormField
                    id="budget-fixed-value"
                    label={t('organizationBudgets.form.fixedValue')}
                    error={form.formState.errors.fixedValue?.message}
                  >
                    <Input
                      id="budget-fixed-value"
                      inputMode="numeric"
                      aria-invalid={!!form.formState.errors.fixedValue}
                      {...form.register('fixedValue')}
                    />
                  </FormField>
                  <FormField id="budget-fixed-unit" label={t('organizationBudgets.form.fixedUnit')}>
                    <Select id="budget-fixed-unit" {...form.register('fixedUnit')}>
                      <option value="seconds">{t('organizationBudgets.fixedUnit.seconds')}</option>
                      <option value="minutes">{t('organizationBudgets.fixedUnit.minutes')}</option>
                      <option value="hours">{t('organizationBudgets.fixedUnit.hours')}</option>
                      <option value="days">{t('organizationBudgets.fixedUnit.days')}</option>
                    </Select>
                  </FormField>
                </div>
              ) : null}
              <p className="text-[0.6875rem] leading-4 text-muted-foreground">
                {t('organizationBudgets.form.utcHint')}
              </p>
            </section>

            <section className="grid gap-3" aria-labelledby="budget-limit-fields">
              <h3 id="budget-limit-fields" className="text-xs font-semibold">
                {t('organizationBudgets.form.limitSection')}
              </h3>
              <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
                <div className="min-w-0">
                  <Label htmlFor="budget-unlimited">{t('organizationBudgets.form.unlimited')}</Label>
                  <p className="mt-0.5 text-[0.6875rem] leading-4 text-muted-foreground">
                    {t('organizationBudgets.form.unlimitedHint')}
                  </p>
                </div>
                <Switch
                  id="budget-unlimited"
                  checked={unlimited}
                  onCheckedChange={(checked) => form.setValue('unlimited', checked, { shouldValidate: true })}
                />
              </div>
              <FormField
                id="budget-quota-limit"
                label={t('organizationBudgets.form.quotaLimit')}
                error={form.formState.errors.quotaLimit?.message}
              >
                <Input
                  id="budget-quota-limit"
                  inputMode="numeric"
                  disabled={unlimited}
                  aria-invalid={!!form.formState.errors.quotaLimit}
                  {...form.register('quotaLimit')}
                />
              </FormField>
            </section>

            <section className="grid gap-3" aria-labelledby="budget-effective-fields">
              <h3 id="budget-effective-fields" className="text-xs font-semibold">
                {t('organizationBudgets.form.effectiveSection')}
              </h3>
              <FormField
                id="budget-effective-from"
                label={t('organizationBudgets.form.effectiveFrom')}
                error={form.formState.errors.effectiveFrom?.message}
                hint={latestEffectiveFrom === undefined
                  ? t('organizationBudgets.form.effectiveHint')
                  : t('organizationBudgets.form.latestEffective', {
                    value: new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' })
                      .format(latestEffectiveFrom * 1_000),
                  })}
              >
                <Input
                  id="budget-effective-from"
                  type="datetime-local"
                  step={60}
                  aria-invalid={!!form.formState.errors.effectiveFrom}
                  {...form.register('effectiveFrom')}
                />
              </FormField>
            </section>

            {errorCode ? (
              <MutationError
                code={errorCode}
                refreshing={verifying}
                onRefresh={() => void refreshAfterError()}
              />
            ) : null}
          </div>

          <div className="flex items-center justify-end gap-2 border-t border-[var(--hairline)] p-4">
            <Button type="button" variant="secondary" onClick={() => onOpenChange(false)}>
              {t('organizationBudgets.actions.cancel')}
            </Button>
            <Button type="submit" disabled={mutation.isPending || outcomeUnknown || directoriesPending}>
              {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
              {t(mutation.isPending ? 'organizationBudgets.actions.creating' : 'organizationBudgets.actions.create')}
            </Button>
          </div>
        </form>
      </SheetContent>
    </Sheet>
  )
}

function FormField({
  id,
  label,
  hint,
  error,
  children,
}: {
  id: string
  label: string
  hint?: string
  error?: string
  children: ReactNode
}) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-[0.6875rem] leading-4 text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}

function MutationError({
  code,
  refreshing,
  onRefresh,
}: {
  code: string
  refreshing: boolean
  onRefresh: () => void
}) {
  const { t } = useTranslation()
  const needsRefresh = code === 'budget_conflict' || code === 'budget_outcome_unknown'
  return (
    <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-3">
      <div className="flex gap-2">
        <AlertTriangle className="mt-0.5 size-4 shrink-0 text-destructive" aria-hidden="true" />
        <div className="min-w-0">
          <p className="text-xs font-semibold text-destructive">
            {t(`organizationBudgets.errors.mutation.${code}.title`, {
              defaultValue: t('organizationBudgets.errors.mutation.unknown.title'),
            })}
          </p>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
            {t(`organizationBudgets.errors.mutation.${code}.body`, {
              defaultValue: t('organizationBudgets.errors.mutation.unknown.body'),
            })}
          </p>
          {needsRefresh ? (
            <Button type="button" size="sm" variant="secondary" className="mt-3" disabled={refreshing} onClick={onRefresh}>
              <RefreshCw className={refreshing ? 'animate-spin' : undefined} aria-hidden="true" />
              {t('organizationBudgets.actions.verify')}
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  )
}
