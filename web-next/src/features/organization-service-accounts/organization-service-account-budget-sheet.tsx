import { Button, Drawer, DrawerBody, DrawerContent, DrawerFooter, DrawerHeader, Select, SelectItem, Skeleton } from '@heroui/react'
import { LoaderCircle, Save, WalletCards } from 'lucide-react'
import type { TFunction } from 'i18next'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type {
  OrganizationBudgetPolicyResponse,
  OrganizationServiceAccountResponse,
  OrganizationTeamResponse,
} from '@/lib/api/generated/types.gen'
import {
  organizationServiceAccountErrorStatus,
  useOrganizationServiceAccountBudgetBinding,
  useSetOrganizationServiceAccountBudgetBinding,
} from './organization-service-account-api'

export function OrganizationServiceAccountBudgetSheet({
  organizationId,
  account,
  policies,
  teams,
  canManage,
  open,
  onOpenChange,
}: {
  organizationId: number
  account?: OrganizationServiceAccountResponse
  policies: OrganizationBudgetPolicyResponse[]
  teams: OrganizationTeamResponse[]
  canManage: boolean
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const accountKey = account?.service_account_key
  const bindingQuery = useOrganizationServiceAccountBudgetBinding(organizationId, accountKey, open)
  const mutation = useSetOrganizationServiceAccountBudgetBinding(organizationId)
  const resetMutation = mutation.reset
  const [policyId, setPolicyId] = useState('')
  const teamNames = useMemo(() => new Map(teams.map((team) => [team.id, team.name])), [teams])
  const eligiblePolicies = useMemo(() => {
    const now = Math.floor(Date.now() / 1_000)
    return policies
      .filter((policy) => policy.effective_from <= now && (policy.effective_until === null || policy.effective_until > now))
      .filter((policy) => policy.scope.kind === 'organization' || policy.scope.kind === 'team')
      .sort((left, right) => right.effective_from - left.effective_from || right.version - left.version)
  }, [policies])

  useEffect(() => {
    if (!open) return
    setPolicyId(bindingQuery.data ? String(bindingQuery.data.policy_id) : '')
    resetMutation()
  }, [bindingQuery.data, open, accountKey, resetMutation])

  const save = async () => {
    if (!accountKey || !policyId) return
    try {
      await mutation.mutateAsync({
        serviceAccountKey: accountKey,
        body: {
          policy_id: Number(policyId),
          expected_version: bindingQuery.data?.version ?? null,
        },
      })
      onOpenChange(false)
    } catch {
      // 版本冲突时保留选择，管理员可刷新事实后再决定是否提交。
    }
  }

  const queryError = bindingQuery.isError
  const conflict = organizationServiceAccountErrorStatus(mutation.error) === 409
  const errorText = conflict
    ? t('organizationServiceAccounts.budget.conflict')
    : t('organizationServiceAccounts.budget.saveError')

  return (
    <Drawer
      backdrop="blur"
      classNames={{ base: 'w-full max-h-none sm:max-w-lg' }}
      isOpen={open}
      placement="right"
      scrollBehavior="inside"
      onOpenChange={onOpenChange}
    >
      <DrawerContent>
        {() => (
          <>
            <DrawerHeader className="flex flex-col gap-0.5 border-b border-[var(--hairline)] p-4 pr-12">
              <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand"><WalletCards className="size-3.5" aria-hidden="true" />{t('organizationServiceAccounts.budget.eyebrow')}</div>
              <h2 className="text-base font-medium text-foreground">{t('organizationServiceAccounts.budget.title')}</h2>
              <p className="text-sm text-muted-foreground">{t('organizationServiceAccounts.budget.description', { name: account?.name })}</p>
            </DrawerHeader>
            <DrawerBody className="min-h-0 flex-1 gap-0 overflow-hidden p-0">
              <div className="flex min-h-0 flex-1 flex-col">
                <div className="flex-1 overflow-y-auto p-4">
                  {!canManage ? (
                    <p className="rounded-lg bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">{t('organizationServiceAccounts.budget.permission')}</p>
                  ) : bindingQuery.isPending ? (
                    <div className="grid gap-2" role="status"><Skeleton className="h-8 rounded-lg" /><Skeleton className="h-16 rounded-lg" /></div>
                  ) : queryError ? (
                    <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t('organizationServiceAccounts.budget.loadError')}</p>
                  ) : (
                    <div className="grid gap-4">
                      <div className="grid gap-1.5">
                        <label className="text-xs font-medium" htmlFor="organization-service-account-budget-policy">{t('organizationServiceAccounts.budget.policy')}</label>
                        <Select
                          aria-label={t('organizationServiceAccounts.budget.policy')}
                          id="organization-service-account-budget-policy"
                          isDisabled={mutation.isPending || eligiblePolicies.length === 0}
                          items={[
                            { key: '', label: eligiblePolicies.length === 0 ? t('organizationServiceAccounts.budget.noPolicies') : t('organizationServiceAccounts.budget.selectPolicy') },
                            ...eligiblePolicies.map((policy) => ({ key: String(policy.id), label: policyLabel(policy, teamNames, t) })),
                          ]}
                          selectedKeys={policyId === '' ? [] : [policyId]}
                          size="sm"
                          onSelectionChange={(keys) => { setPolicyId(String(Array.from(keys)[0] ?? '')); resetMutation() }}
                        >
                          {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                        </Select>
                        <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('organizationServiceAccounts.budget.policyHint')}</p>
                      </div>
                      <div className="rounded-lg border border-[var(--hairline)] bg-surface-2/45 px-3 py-2.5 text-xs leading-5 text-muted-foreground">
                        {bindingQuery.data
                          ? t('organizationServiceAccounts.budget.current', { version: bindingQuery.data.version, policyVersion: bindingQuery.data.policy_version })
                          : t('organizationServiceAccounts.budget.unbound')}
                      </div>
                      {mutation.isError ? <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{errorText}</p> : null}
                    </div>
                  )}
                </div>
                <DrawerFooter className="border-t border-[var(--hairline)]">
                  <Button type="button" variant="bordered" isDisabled={mutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationServiceAccounts.actions.cancel')}</Button>
                  <Button type="button" color="primary" isDisabled={!canManage || bindingQuery.isPending || queryError || !policyId || mutation.isPending} onClick={() => void save()}>
                    {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Save className="size-4" aria-hidden="true" />}
                    {t('organizationServiceAccounts.actions.saveBudget')}
                  </Button>
                </DrawerFooter>
              </div>
            </DrawerBody>
          </>
        )}
      </DrawerContent>
    </Drawer>
  )
}

function policyLabel(
  policy: OrganizationBudgetPolicyResponse,
  teamNames: ReadonlyMap<number, string>,
  t: TFunction,
) {
  const scope = policy.scope.kind === 'team'
    ? t('organizationServiceAccounts.budget.teamPolicy', { team: teamNames.get(policy.scope.team_id) ?? policy.scope.team_id })
    : t('organizationServiceAccounts.budget.organizationPolicy')
  const period = policy.period.kind === 'fixed'
    ? t('organizationServiceAccounts.budget.fixedPeriod', { seconds: policy.period.seconds })
    : t(`organizationBudgets.period.${policy.period.kind}`)
  return `${scope} · ${period} · v${policy.version}`
}
