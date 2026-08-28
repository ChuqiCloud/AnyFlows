import { LoaderCircle, Save, WalletCards } from 'lucide-react'
import type { TFunction } from 'i18next'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
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
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent className="w-full gap-0 sm:max-w-lg">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand"><WalletCards className="size-3.5" aria-hidden="true" />{t('organizationServiceAccounts.budget.eyebrow')}</div>
          <SheetTitle>{t('organizationServiceAccounts.budget.title')}</SheetTitle>
          <SheetDescription>{t('organizationServiceAccounts.budget.description', { name: account?.name })}</SheetDescription>
        </SheetHeader>
        <div className="flex min-h-0 flex-1 flex-col">
          <div className="flex-1 overflow-y-auto p-4">
            {!canManage ? (
              <p className="rounded-lg bg-warning/8 px-3 py-2 text-xs leading-5 text-warning">{t('organizationServiceAccounts.budget.permission')}</p>
            ) : bindingQuery.isPending ? (
              <div className="grid gap-2" role="status"><div className="h-8 animate-pulse rounded-lg bg-surface-2" /><div className="h-16 animate-pulse rounded-lg bg-surface-2" /></div>
            ) : queryError ? (
              <p role="alert" className="rounded-lg bg-destructive/8 px-3 py-2 text-xs leading-5 text-destructive">{t('organizationServiceAccounts.budget.loadError')}</p>
            ) : (
              <div className="grid gap-4">
                <div className="grid gap-1.5">
                  <label htmlFor="organization-service-account-budget-policy" className="text-xs font-medium">{t('organizationServiceAccounts.budget.policy')}</label>
                  <Select
                    id="organization-service-account-budget-policy"
                    value={policyId}
                    disabled={mutation.isPending || eligiblePolicies.length === 0}
        onChange={(event) => { setPolicyId(event.target.value); resetMutation() }}
                  >
                    <option value="">{eligiblePolicies.length === 0 ? t('organizationServiceAccounts.budget.noPolicies') : t('organizationServiceAccounts.budget.selectPolicy')}</option>
                    {eligiblePolicies.map((policy) => (
                      <option key={policy.id} value={policy.id}>
                        {policyLabel(policy, teamNames, t)}
                      </option>
                    ))}
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
          <SheetFooter className="border-t border-[var(--hairline)]">
            <Button type="button" variant="outline" disabled={mutation.isPending} onClick={() => onOpenChange(false)}>{t('organizationServiceAccounts.actions.cancel')}</Button>
            <Button type="button" disabled={!canManage || bindingQuery.isPending || queryError || !policyId || mutation.isPending} onClick={() => void save()}>
              {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Save aria-hidden="true" />}
              {t('organizationServiceAccounts.actions.saveBudget')}
            </Button>
          </SheetFooter>
        </div>
      </SheetContent>
    </Sheet>
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
