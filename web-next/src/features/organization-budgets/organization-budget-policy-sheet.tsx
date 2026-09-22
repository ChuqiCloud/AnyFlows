import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Drawer, DrawerBody, DrawerContent, DrawerHeader, Input, Select, SelectItem, Switch } from '@heroui/react'
import { AlertTriangle, LoaderCircle, RefreshCw } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
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

const SCOPE_KIND_ITEMS = ['organization', 'team', 'member'] as const
const PERIOD_KIND_ITEMS = ['utc_day', 'utc_week', 'utc_month', 'fixed'] as const
const FIXED_UNIT_ITEMS = ['seconds', 'minutes', 'hours', 'days'] as const

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
  const scopeKindItems = SCOPE_KIND_ITEMS.map((kind) => ({ key: kind, label: t(`organizationBudgets.scope.${kind}`) }))
  const periodKindItems = PERIOD_KIND_ITEMS.map((kind) => ({ key: kind, label: t(`organizationBudgets.period.${kind}`) }))
  const fixedUnitItems = FIXED_UNIT_ITEMS.map((unit) => ({ key: unit, label: t(`organizationBudgets.fixedUnit.${unit}`) }))
  // HeroUI Select 用哨兵 key 表达"未选择"，空字符串 key 不被接受。
  const teamItems = activeTeams.map((team) => ({ key: String(team.id), label: team.name }))
  const memberItems = activeMembers.map((member) => ({
    key: String(member.id),
    label: t('organizationBudgets.memberOption', {
      membershipId: member.id,
      userId: member.user_id,
      role: t(`organizationBudgets.roles.${member.role}`, { defaultValue: member.role }),
    }),
  }))

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
    <Drawer
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-xl' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <h2 className="text-base font-medium text-foreground">{t('organizationBudgets.form.title')}</h2>
              <p className="text-sm text-muted-foreground">{t('organizationBudgets.form.subtitle')}</p>
            </DrawerHeader>

            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
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
                      <Controller
                        control={form.control}
                        name="scopeKind"
                        render={({ field }) => (
                          <Select
                            aria-label={t('organizationBudgets.form.scopeKind')}
                            id="budget-scope-kind"
                            items={scopeKindItems}
                            selectedKeys={[field.value]}
                            onSelectionChange={(keys) => field.onChange(Array.from(keys)[0])}
                          >
                            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                          </Select>
                        )}
                      />
                    </FormField>

                    {scopeKind === 'team' ? (
                      <FormField
                        id="budget-team-id"
                        label={t('organizationBudgets.form.team')}
                        error={form.formState.errors.teamId?.message}
                        hint={directoriesError ? t('organizationBudgets.form.directoryError') : undefined}
                      >
                        <Controller
                          control={form.control}
                          name="teamId"
                          render={({ field }) => (
                            <Select
                              aria-label={t('organizationBudgets.form.team')}
                              id="budget-team-id"
                              isDisabled={directoriesPending || directoriesError}
                              isInvalid={!!form.formState.errors.teamId}
                              items={teamItems}
                              placeholder={t(directoriesPending ? 'organizationBudgets.form.directoryLoading' : 'organizationBudgets.form.selectTeam')}
                              selectedKeys={field.value === '' ? [] : [field.value]}
                              onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? ''))}
                            >
                              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                            </Select>
                          )}
                        />
                      </FormField>
                    ) : null}

                    {scopeKind === 'member' ? (
                      <FormField
                        id="budget-membership-id"
                        label={t('organizationBudgets.form.member')}
                        error={form.formState.errors.membershipId?.message}
                        hint={directoriesError ? t('organizationBudgets.form.directoryError') : undefined}
                      >
                        <Controller
                          control={form.control}
                          name="membershipId"
                          render={({ field }) => (
                            <Select
                              aria-label={t('organizationBudgets.form.member')}
                              id="budget-membership-id"
                              isDisabled={directoriesPending || directoriesError}
                              isInvalid={!!form.formState.errors.membershipId}
                              items={memberItems}
                              placeholder={t(directoriesPending ? 'organizationBudgets.form.directoryLoading' : 'organizationBudgets.form.selectMember')}
                              selectedKeys={field.value === '' ? [] : [field.value]}
                              onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? ''))}
                            >
                              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                            </Select>
                          )}
                        />
                      </FormField>
                    ) : null}
                  </section>

                  <section className="grid gap-3" aria-labelledby="budget-period-fields">
                    <h3 id="budget-period-fields" className="text-xs font-semibold">
                      {t('organizationBudgets.form.periodSection')}
                    </h3>
                    <FormField id="budget-period-kind" label={t('organizationBudgets.form.periodKind')}>
                      <Controller
                        control={form.control}
                        name="periodKind"
                        render={({ field }) => (
                          <Select
                            aria-label={t('organizationBudgets.form.periodKind')}
                            id="budget-period-kind"
                            items={periodKindItems}
                            selectedKeys={[field.value]}
                            onSelectionChange={(keys) => field.onChange(Array.from(keys)[0])}
                          >
                            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                          </Select>
                        )}
                      />
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
                            isInvalid={!!form.formState.errors.fixedValue}
                            size="sm"
                            {...form.register('fixedValue')}
                          />
                        </FormField>
                        <FormField id="budget-fixed-unit" label={t('organizationBudgets.form.fixedUnit')}>
                          <Controller
                            control={form.control}
                            name="fixedUnit"
                            render={({ field }) => (
                              <Select
                                aria-label={t('organizationBudgets.form.fixedUnit')}
                                id="budget-fixed-unit"
                                items={fixedUnitItems}
                                selectedKeys={[field.value]}
                                onSelectionChange={(keys) => field.onChange(Array.from(keys)[0])}
                              >
                                {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                              </Select>
                            )}
                          />
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
                        <label className="text-xs font-medium leading-none text-foreground" htmlFor="budget-unlimited">{t('organizationBudgets.form.unlimited')}</label>
                        <p className="mt-0.5 text-[0.6875rem] leading-4 text-muted-foreground">
                          {t('organizationBudgets.form.unlimitedHint')}
                        </p>
                      </div>
                      <Switch
                        id="budget-unlimited"
                        isSelected={unlimited}
                        size="sm"
                        onValueChange={(checked) => form.setValue('unlimited', checked, { shouldValidate: true })}
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
                        isDisabled={unlimited}
                        isInvalid={!!form.formState.errors.quotaLimit}
                        size="sm"
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
                        isInvalid={!!form.formState.errors.effectiveFrom}
                        size="sm"
                        step={60}
                        type="datetime-local"
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
                  <Button type="button" variant="bordered" onClick={() => onOpenChange(false)}>
                    {t('organizationBudgets.actions.cancel')}
                  </Button>
                  <Button type="submit" color="primary" isDisabled={mutation.isPending || outcomeUnknown || directoriesPending}>
                    {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
                    {t(mutation.isPending ? 'organizationBudgets.actions.creating' : 'organizationBudgets.actions.create')}
                  </Button>
                </div>
              </form>
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
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
      {/* HeroUI 没有独立 Label 组件，表单标签保留原生元素以维持原有的标签/提示/错误层级。 */}
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
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
            <Button type="button" size="sm" variant="bordered" className="mt-3" isDisabled={refreshing} onClick={onRefresh}>
              <RefreshCw className={cn('size-3.5', refreshing && 'animate-spin')} aria-hidden="true" />
              {t('organizationBudgets.actions.verify')}
            </Button>
          ) : null}
        </div>
      </div>
    </div>
  )
}
