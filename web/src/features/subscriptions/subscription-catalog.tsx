import { BadgeDollarSign, CreditCard, Layers3, RefreshCw } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { SubscriptionCatalogPlan } from '@/lib/api/generated/types.gen'
import { useCurrentSubscriptionCatalog } from './subscription-api'

/** 展示服务端已筛选的可售计划；本阶段只读取目录，不发起订单或支付。 */
export function SubscriptionCatalog() {
  const { i18n, t } = useTranslation()
  const query = useCurrentSubscriptionCatalog()
  const { formatQuota } = useBalanceDisplay()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const numberFormat = useMemo(() => new Intl.NumberFormat(locale), [locale])

  return (
    <section className="border-t border-[var(--hairline)] pt-5" aria-labelledby="subscription-catalog-title">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <div className="mb-1 flex items-center gap-2 text-[0.6875rem] text-brand">
            <BadgeDollarSign className="size-3.5" aria-hidden="true" />
            {t('subscriptions.catalog.eyebrow')}
          </div>
          <h3 id="subscription-catalog-title" className="text-sm font-semibold">
            {t('subscriptions.catalog.title')}
          </h3>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('subscriptions.catalog.description')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={query.isFetching}
          onClick={() => void query.refetch()}
        >
          <RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />
          {t('subscriptions.actions.refresh')}
        </Button>
      </header>

      {query.isPending ? (
        <div className="mt-4 grid gap-2" aria-label={t('subscriptions.catalog.loading')}>
          {[0, 1].map((item) => <Skeleton key={item} className="h-24 rounded-lg" />)}
        </div>
      ) : null}

      {query.isError && query.data === undefined ? (
        <div role="alert" className="mt-4 rounded-lg border border-destructive/25 bg-destructive/8 p-4">
          <h4 className="text-sm font-semibold text-destructive">{t('subscriptions.errors.catalogTitle')}</h4>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t('subscriptions.errors.catalogBody')}
          </p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void query.refetch()}>
            <RefreshCw aria-hidden="true" />
            {t('subscriptions.actions.retry')}
          </Button>
        </div>
      ) : null}

      {query.data !== undefined && query.data.plans.length === 0 ? (
        <div className="mt-4 grid min-h-44 place-items-center border-y border-[var(--hairline)] py-8 text-center">
          <div className="max-w-sm">
            <span className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
              <Layers3 className="size-4" aria-hidden="true" />
            </span>
            <h4 className="mt-3 text-sm font-semibold">{t('subscriptions.empty.catalogTitle')}</h4>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('subscriptions.empty.catalogBody')}
            </p>
          </div>
        </div>
      ) : null}

      {query.data !== undefined && query.data.plans.length > 0 ? (
        <div className="mt-4 grid gap-2">
          {query.data.plans.map((plan) => (
            <CatalogPlanRow
              key={plan.plan_id}
              plan={plan}
              formatQuota={formatQuota}
              formatAmount={(value) => numberFormat.format(value)}
            />
          ))}
        </div>
      ) : null}

      <div className="mt-3 flex items-start gap-2 border-y border-[var(--hairline)] py-3 text-xs leading-5 text-muted-foreground">
        <CreditCard className="mt-0.5 size-3.5 shrink-0 text-brand" aria-hidden="true" />
        <span>{t('subscriptions.catalog.orderNotice')}</span>
      </div>
    </section>
  )
}

function CatalogPlanRow({
  plan,
  formatQuota,
  formatAmount,
}: {
  plan: SubscriptionCatalogPlan
  formatQuota: (value: number) => string
  formatAmount: (value: number) => string
}) {
  const { t } = useTranslation()

  return (
    <article className="grid gap-3 rounded-lg border border-[var(--hairline)] px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="min-w-0">
        <h4 className="truncate text-sm font-medium">{plan.name}</h4>
        <p className="mt-1 text-xs text-muted-foreground">
          {t('subscriptions.catalog.planQuota', {
            quota: formatQuota(plan.quota_amount),
            cycle: t(`subscriptions.cycle.${plan.cycle}`),
          })}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <Badge className="bg-surface-2 text-muted-foreground">{plan.price_provider}</Badge>
        <span className="font-mono tabular-nums text-foreground">
          {t('subscriptions.catalog.priceMinor', {
            currency: plan.price_currency,
            amount: formatAmount(plan.price_amount_minor),
          })}
        </span>
      </div>
    </article>
  )
}
