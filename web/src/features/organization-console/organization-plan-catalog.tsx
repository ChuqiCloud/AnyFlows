import {
  BadgeDollarSign,
  Check,
  Clock3,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  UsersRound,
} from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import type { OrganizationPlan } from '@/lib/api/generated/types.gen'

import { useOrganizationPlanCatalog } from './organization-plan-api'

/** 企业控制台只展示服务端可售快照，不把目录误报为已购买授权。 */
export function OrganizationPlanCatalog() {
  const { t, i18n } = useTranslation()
  const query = useOrganizationPlanCatalog()
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
        <Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}>
          <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
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
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void query.refetch()}>
            <RefreshCw aria-hidden="true" />
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
            <PlanCard key={plan.plan_id} plan={plan} numberFormat={numberFormat} />
          ))}
        </div>
      ) : null}

      {query.data !== undefined ? (
        <p className="border-y border-[var(--hairline)] py-3 text-xs leading-5 text-muted-foreground">
          {query.data.purchase_notice}
        </p>
      ) : null}
    </section>
  )
}

function PlanCard({ plan, numberFormat }: { plan: OrganizationPlan; numberFormat: Intl.NumberFormat }) {
  const { t } = useTranslation()
  const features = [
    { icon: UsersRound, label: t('organizationConsole.overview.planCatalog.seats', { count: numberFormat.format(plan.seat_limit) }) },
    { icon: ShieldCheck, label: t('organizationConsole.overview.planCatalog.teams', { count: numberFormat.format(plan.team_limit) }) },
    { icon: KeyRound, label: t('organizationConsole.overview.planCatalog.keys', { count: numberFormat.format(plan.organization_key_limit) }) },
    { icon: Clock3, label: t('organizationConsole.overview.planCatalog.auditRetention', { count: numberFormat.format(plan.audit_retention_days) }) },
  ]
  return (
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <CardTitle className="truncate text-base">{plan.name}</CardTitle>
            {plan.description ? <p className="mt-1 text-xs leading-5 text-muted-foreground">{plan.description}</p> : null}
          </div>
          <Badge className="shrink-0 bg-brand/10 text-brand">{plan.price_provider}</Badge>
        </div>
      </CardHeader>
      <CardContent className="grid gap-4 p-4">
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
      </CardContent>
    </Card>
  )
}

function Capability({ enabled, label }: { enabled: boolean; label: string }) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center gap-2">
      <Check className={`size-3.5 ${enabled ? 'text-success' : 'text-muted-foreground/45'}`} aria-hidden="true" />
      <span className={enabled ? 'text-foreground' : 'text-muted-foreground'}>{label}</span>
      <span className="sr-only">{enabled ? t('organizationConsole.overview.planCatalog.enabled') : t('organizationConsole.overview.planCatalog.disabled')}</span>
    </div>
  )
}
