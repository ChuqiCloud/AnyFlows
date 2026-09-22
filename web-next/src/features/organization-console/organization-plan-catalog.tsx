import {
  BadgeDollarSign,
  Check,
  Clock3,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  UsersRound,
} from 'lucide-react'
import { Button, Card, CardBody, CardHeader, Chip } from '@heroui/react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import type { UserTopupMethod } from '@/features/payment-settings/payment-settings-types'
import { useUserTopupConfiguration } from '@/features/wallet/wallet-api'
import type { OrganizationEntitlementProjection, OrganizationPlan, OrganizationPlanOrderKind } from '@/lib/api/generated/types.gen'

import { useOrganizationPlanCatalog } from './organization-plan-api'
import { OrganizationPlanPurchase } from './organization-plan-purchase'

/** 企业控制台展示服务端可售快照并绑定当前企业，不把目录误报为已购买授权。 */
export function OrganizationPlanCatalog({
  organizationId,
  canPurchase,
  entitlement,
  entitlementRefreshing,
  onRefreshEntitlement,
}: {
  organizationId: number
  canPurchase: boolean
  entitlement?: OrganizationEntitlementProjection
  entitlementRefreshing: boolean
  onRefreshEntitlement?: () => Promise<unknown>
}) {
  const { t, i18n } = useTranslation()
  const query = useOrganizationPlanCatalog()
  const configurationQuery = useUserTopupConfiguration(canPurchase)
  const numberFormat = useMemo(
    () => new Intl.NumberFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'),
    [i18n.resolvedLanguage],
  )

  return (
    <section aria-labelledby="organization-plan-catalog-title" className="grid gap-3">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <BadgeDollarSign className="size-3.5" aria-hidden="true" />
            {t('organizationConsole.overview.planCatalog.eyebrow')}
          </div>
          <h3 id="organization-plan-catalog-title" className="text-base font-semibold">
            {t('organizationConsole.overview.planCatalog.title')}
          </h3>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('organizationConsole.overview.planCatalog.subtitle')}
          </p>
        </div>
        <Button type="button" size="sm" variant="bordered" isDisabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={query.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
          {t('organizationConsole.actions.refresh')}
        </Button>
      </header>

      {query.isPending ? (
        <div className="grid gap-3 sm:grid-cols-2" aria-label={t('organizationConsole.overview.planCatalog.loading')}>
          {[0, 1].map((item) => <div key={item} className="h-56 animate-pulse rounded-lg border border-[var(--hairline)] bg-surface-1/45" />)}
        </div>
      ) : null}

      {query.isError && query.data === undefined ? (
        <div role="alert" className="rounded-lg border border-destructive/25 bg-destructive/8 p-4">
          <h4 className="text-sm font-semibold text-destructive">{t('organizationConsole.overview.planCatalog.errorTitle')}</h4>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.overview.planCatalog.errorBody')}</p>
          <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void query.refetch()}>
            <RefreshCw aria-hidden="true" className="size-3.5" />
            {t('organizationConsole.actions.retry')}
          </Button>
        </div>
      ) : null}

      {query.data !== undefined && query.data.plans.length === 0 ? (
        <div className="grid min-h-36 place-items-center rounded-lg border border-dashed border-[var(--hairline)] px-4 py-8 text-center">
          <p className="max-w-md text-xs leading-5 text-muted-foreground">{t('organizationConsole.overview.planCatalog.empty')}</p>
        </div>
      ) : null}

      {query.data !== undefined && query.data.plans.length > 0 ? (
        <div className="grid gap-3 lg:grid-cols-2">
          {query.data.plans.map((plan) => (
            <PlanCard
              key={plan.plan_id}
              organizationId={organizationId}
              plan={plan}
              numberFormat={numberFormat}
              methods={configurationQuery.data?.methods}
              methodsLoading={configurationQuery.isPending && canPurchase}
              methodsUnavailable={configurationQuery.isError && canPurchase}
              canPurchase={canPurchase}
              entitlement={entitlement}
              entitlementRefreshing={entitlementRefreshing}
              onRefreshEntitlement={onRefreshEntitlement}
              orderKind={planOrderKind(plan, entitlement)}
            />
          ))}
        </div>
      ) : null}

      {query.data !== undefined ? (
        <p className="border-t border-[var(--hairline)] py-3 text-xs leading-5 text-muted-foreground">
          {query.data.purchase_notice}
        </p>
      ) : null}
    </section>
  )
}

function PlanCard({
  organizationId,
  plan,
  numberFormat,
  methods,
  methodsLoading,
  methodsUnavailable,
  canPurchase,
  entitlement,
  entitlementRefreshing,
  onRefreshEntitlement,
  orderKind,
}: {
  organizationId: number
  plan: OrganizationPlan
  numberFormat: Intl.NumberFormat
  methods: UserTopupMethod[] | undefined
  methodsLoading: boolean
  methodsUnavailable: boolean
  canPurchase: boolean
  entitlement?: OrganizationEntitlementProjection
  entitlementRefreshing: boolean
  onRefreshEntitlement?: () => Promise<unknown>
  orderKind: OrganizationPlanOrderKind
}) {
  const { t } = useTranslation()
  const features = [
    { icon: UsersRound, label: t('organizationConsole.overview.planCatalog.seats', { count: numberFormat.format(plan.seat_limit) }) },
    { icon: ShieldCheck, label: t('organizationConsole.overview.planCatalog.teams', { count: numberFormat.format(plan.team_limit) }) },
    { icon: KeyRound, label: t('organizationConsole.overview.planCatalog.keys', { count: numberFormat.format(plan.organization_key_limit) }) },
    { icon: Clock3, label: t('organizationConsole.overview.planCatalog.auditRetention', { count: numberFormat.format(plan.audit_retention_days) }) },
  ]
  return (
    <Card shadow="none" className="overflow-hidden border border-[var(--hairline)] bg-card">
      <CardHeader className="block border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <h3 className="truncate text-base font-semibold">{plan.name}</h3>
            {plan.description ? <p className="mt-1 text-xs leading-5 text-muted-foreground">{plan.description}</p> : null}
          </div>
          <div className="flex shrink-0 flex-wrap justify-end gap-2">
            {entitlement?.plan_id === plan.plan_id ? (
              <Chip className="bg-success/10 text-success" size="sm" variant="flat">{t('organizationConsole.overview.planCatalog.currentEntitlement')}</Chip>
            ) : null}
            <Chip className="bg-brand/10 text-brand" size="sm" variant="flat">{plan.price_provider}</Chip>
          </div>
        </div>
      </CardHeader>
      <CardBody className="grid gap-4 p-4">
        <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
          <span className="font-mono text-xl font-semibold tabular-nums">{plan.price_currency} {numberFormat.format(plan.price_amount_minor)}</span>
          <span className="text-[0.6875rem] text-muted-foreground">{t('organizationConsole.overview.planCatalog.minorUnit')}</span>
        </div>
        <div className="grid gap-2 sm:grid-cols-2">
          {features.map(({ icon: Icon, label }) => (
            <div key={label} className="flex items-center gap-2 text-xs text-muted-foreground">
              <Icon className="size-3.5 shrink-0 text-brand" aria-hidden="true" />
              <span>{label}</span>
            </div>
          ))}
        </div>
        <div className="grid gap-2 border-t border-[var(--hairline)] pt-3 text-xs">
          <Capability enabled={plan.team_management_enabled} label={t('organizationConsole.overview.planCatalog.teamManagement')} />
          <Capability enabled={plan.sso_enabled} label={t('organizationConsole.overview.planCatalog.sso')} />
          <Capability enabled={plan.custom_roles_enabled} label={t('organizationConsole.overview.planCatalog.customRoles')} />
          <span className="text-muted-foreground">{t('organizationConsole.overview.planCatalog.validity', { count: numberFormat.format(plan.validity_days) })}</span>
        </div>
        <OrganizationPlanPurchase
          organizationId={organizationId}
          plan={plan}
          methods={methods}
          methodsLoading={methodsLoading}
          methodsUnavailable={methodsUnavailable}
          canPurchase={canPurchase}
          entitlement={entitlement}
          entitlementRefreshing={entitlementRefreshing}
          onRefreshEntitlement={onRefreshEntitlement}
          orderKind={orderKind}
        />
      </CardBody>
    </Card>
  )
}

function planOrderKind(
  plan: OrganizationPlan,
  entitlement?: OrganizationEntitlementProjection,
): OrganizationPlanOrderKind {
  if (entitlement?.plan_id === undefined || entitlement.plan_id === null) return 'initial'
  if (entitlement.plan_id === plan.plan_id) return 'renewal'
  const currentDays = Math.max(1, Math.floor((entitlement.valid_until - entitlement.valid_from) / 86_400))
  const expandsCapacity = plan.seat_limit >= entitlement.seat_limit
    && plan.team_limit >= entitlement.team_limit
    && plan.organization_key_limit >= entitlement.organization_key_limit
    && plan.validity_days >= currentDays
  const expandsFeatures = (plan.team_management_enabled || !entitlement.team_management_enabled)
    && (plan.sso_enabled || !entitlement.sso_enabled)
    && (plan.custom_roles_enabled || !entitlement.custom_roles_enabled)
  return expandsCapacity && expandsFeatures ? 'upgrade' : 'downgrade'
}

function Capability({ enabled, label }: { enabled: boolean; label: string }) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-2">
      <Check className={enabled ? 'size-3.5 text-success' : 'size-3.5 text-muted-foreground/45'} aria-hidden="true" />
      <span className={enabled ? 'text-foreground' : 'text-muted-foreground'}>{label}</span>
      <span className="sr-only">{enabled ? t('organizationConsole.overview.planCatalog.enabled') : t('organizationConsole.overview.planCatalog.disabled')}</span>
    </div>
  )
}
