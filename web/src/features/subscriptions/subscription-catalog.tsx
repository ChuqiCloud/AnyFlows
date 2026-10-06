import { Elements } from '@stripe/react-stripe-js'
import { loadStripe, type PaymentIntent } from '@stripe/stripe-js'
import { useQueryClient } from '@tanstack/react-query'
import {
  AlertTriangle,
  BadgeDollarSign,
  CheckCircle2,
  CreditCard,
  ExternalLink,
  Landmark,
  Layers3,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  Smartphone,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { PaymentMethod, UserTopupMethod } from '@/features/payment-settings/payment-settings-types'
import { useUserTopupConfiguration } from '@/features/wallet/wallet-api'
import { WalletStripePaymentForm } from '@/features/wallet/wallet-stripe-payment-form'
import { EpayQrPayment } from '@/features/wallet/epay-qr-payment'
import { stripeTopupAppearance } from '@/features/wallet/wallet-topup-runtime'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { SubscriptionCatalogPlan, SubscriptionOrder } from '@/lib/api/generated/types.gen'
import {
  subscriptionErrorCode,
  useCreateCurrentSubscriptionOrder,
  useCurrentSubscriptionCatalog,
  useCurrentSubscriptionOrder,
  useSubmitCurrentSubscriptionOrderPayment,
  currentUserSubscriptionsQueryKey,
} from './subscription-api'
import { createSubscriptionIdempotencyKey } from './subscription-form-model'

type StoredSubscriptionPaymentAttempt = {
  idempotencyKey: string
  orderId?: string
  paymentMethod: PaymentMethod
  planId: string
  provider: string
}

type PaymentPhase = 'processing' | 'succeeded'

/** 展示服务端筛选的可售计划，并把支付动作绑定到同一订阅订单。 */
export function SubscriptionCatalog({
  activePlanIds,
  subscriptionsFetching,
}: {
  activePlanIds: ReadonlySet<string>
  subscriptionsFetching: boolean
}) {
  const { i18n, t } = useTranslation()
  const query = useCurrentSubscriptionCatalog()
  const configurationQuery = useUserTopupConfiguration()
  const { formatQuota } = useBalanceDisplay()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const numberFormat = useMemo(() => new Intl.NumberFormat(locale), [locale])
  const methods = configurationQuery.data?.methods

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
              methods={methods}
              methodsLoading={configurationQuery.isPending}
              methodsUnavailable={configurationQuery.isError}
              active={activePlanIds.has(plan.plan_id)}
              subscriptionsFetching={subscriptionsFetching}
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
  methods,
  methodsLoading,
  methodsUnavailable,
  active,
  subscriptionsFetching,
}: {
  plan: SubscriptionCatalogPlan
  formatQuota: (value: number) => string
  formatAmount: (value: number) => string
  methods: UserTopupMethod[] | undefined
  methodsLoading: boolean
  methodsUnavailable: boolean
  active: boolean
  subscriptionsFetching: boolean
}) {
  const { i18n, t } = useTranslation()
  const createMutation = useCreateCurrentSubscriptionOrder()
  const paymentMutation = useSubmitCurrentSubscriptionOrderPayment()
  const queryClient = useQueryClient()
  const availableMethods = useMemo(
    () => methods?.filter((method) => method.provider === plan.price_provider) ?? [],
    [methods, plan.price_provider],
  )
  const [attempt, setAttempt] = useState(() => readStoredAttempt(plan))
  const [selectedMethod, setSelectedMethod] = useState<PaymentMethod | undefined>(() => attempt?.paymentMethod)
  const [order, setOrder] = useState<SubscriptionOrder>()
  const [payment, setPayment] = useState<{ client_secret: string | null; payment_intent_id: string | null; redirect_url: string | null }>()
  const [phase, setPhase] = useState<PaymentPhase>()
  const orderQuery = useCurrentSubscriptionOrder(attempt?.orderId)
  const errorCode = subscriptionErrorCode(paymentMutation.error) ?? subscriptionErrorCode(createMutation.error)
  const method = availableMethods.find((item) => item.payment_method === selectedMethod) ?? availableMethods[0]
  const stripe = useMemo(() => method?.provider === 'stripe' && method.publishable_key
    ? loadStripe(method.publishable_key)
    : null, [method])
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const amountLabel = useMemo(() => new Intl.NumberFormat(locale, {
    currency: plan.price_currency,
    style: 'currency',
  }).format(plan.price_amount_minor / 100), [locale, plan.price_amount_minor, plan.price_currency])
  const busy = createMutation.isPending || paymentMutation.isPending

  useEffect(() => {
    if (orderQuery.data === undefined) return
    setOrder(orderQuery.data)
    if (orderQuery.data.status === 'paid') {
      void queryClient.invalidateQueries({ queryKey: currentUserSubscriptionsQueryKey })
      if (active) clearStoredAttempt(plan.plan_id)
    }
  }, [active, orderQuery.data, plan.plan_id, queryClient])

  useEffect(() => {
    if (availableMethods.length === 0) return
    setSelectedMethod((current) => availableMethods.some((item) => item.payment_method === current)
      ? current
      : availableMethods[0].payment_method)
  }, [availableMethods])

  const persistAttempt = (next: StoredSubscriptionPaymentAttempt) => {
    setAttempt(next)
    try {
      sessionStorage.setItem(subscriptionAttemptStorageKey(plan.plan_id), JSON.stringify(next))
    } catch {
      // 存储不可用时仍保留当前页面内的幂等重试和支付恢复能力。
    }
  }

  const clearAttempt = () => {
    clearStoredAttempt(plan.plan_id)
    setAttempt(undefined)
    setOrder(undefined)
    setPayment(undefined)
    setPhase(undefined)
    createMutation.reset()
    paymentMutation.reset()
  }

  const submitPayment = async (orderId: string, paymentMethod: PaymentMethod) => {
    const result = await paymentMutation.mutateAsync({
      body: { payment_method: paymentMethod },
      orderId,
    })
    setOrder(result.order)
    setPayment(result.payment)
    setPhase(undefined)
    persistAttempt({
      idempotencyKey: attempt?.idempotencyKey ?? createSubscriptionIdempotencyKey(),
      orderId,
      paymentMethod,
      planId: plan.plan_id,
      provider: plan.price_provider,
    })
    if (result.order.status === 'paid') {
      void queryClient.invalidateQueries({ queryKey: currentUserSubscriptionsQueryKey })
    }
    return result
  }

  const createOrder = async () => {
    if (method === undefined || busy) return
    const idempotencyKey = attempt?.idempotencyKey ?? createSubscriptionIdempotencyKey()
    persistAttempt({
      idempotencyKey,
      paymentMethod: method.payment_method,
      planId: plan.plan_id,
      provider: plan.price_provider,
    })
    const nextOrder = await createMutation.mutateAsync({
      idempotency_key: idempotencyKey,
      plan_id: plan.plan_id,
      plan_version: plan.plan_version,
      price_provider: plan.price_provider,
      price_currency: plan.price_currency,
      price_amount_minor: plan.price_amount_minor,
    })
    setOrder(nextOrder)
    await submitPayment(nextOrder.order_id, method.payment_method)
  }

  const resumePayment = async () => {
    if (attempt?.orderId === undefined || method === undefined || busy) return
    await submitPayment(attempt.orderId, method.payment_method)
  }

  const refreshOrder = async () => {
    const refreshed = await orderQuery.refetch()
    if (refreshed.data?.status === 'paid') {
      await queryClient.invalidateQueries({ queryKey: currentUserSubscriptionsQueryKey })
      if (active) clearStoredAttempt(plan.plan_id)
    }
  }

  const paymentConfirmed = async (status: PaymentIntent.Status) => {
    setPhase(status === 'succeeded' ? 'succeeded' : 'processing')
    await refreshOrder()
  }

  const paymentErrorKey = errorCode === 'subscription_conflict'
    ? 'orderConflict'
    : errorCode === 'subscription_outcome_unknown'
      ? 'paymentOutcomeUnknown'
      : errorCode === 'topup_unavailable'
        ? 'paymentUnavailable'
        : errorCode === 'topup_provider_rejected'
          ? 'paymentRejected'
          : 'paymentSubmit'
  const redirectUrl = payment?.redirect_url ? safePaymentRedirectUrl(payment.redirect_url) : undefined

  return (
    <article className="grid gap-3 rounded-lg border border-[var(--hairline)] px-4 py-3">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
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
      </div>

      {order === undefined ? (
        <div className="grid gap-2 border-t border-[var(--hairline)] pt-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="text-xs font-medium">{t('subscriptions.catalog.paymentMethod')}</span>
            {methodsLoading ? <span className="text-[0.6875rem] text-muted-foreground">{t('subscriptions.catalog.paymentMethodsLoading')}</span> : null}
          </div>
          {methodsUnavailable ? <p role="alert" className="text-xs text-destructive">{t('subscriptions.catalog.paymentMethodsUnavailable')}</p> : null}
          {!methodsLoading && !methodsUnavailable && availableMethods.length === 0 ? <p className="text-xs text-muted-foreground">{t('subscriptions.catalog.paymentMethodsEmpty')}</p> : null}
          {availableMethods.length > 0 ? (
            <div className="grid gap-2 sm:grid-cols-3" role="radiogroup" aria-label={t('subscriptions.catalog.paymentMethod')}>
              {availableMethods.map((item) => {
                const selected = item.payment_method === method?.payment_method
                return (
                  <button
                    key={item.payment_method}
                    type="button"
                    role="radio"
                    aria-checked={selected}
                    className={`flex min-h-12 items-center gap-2 rounded-lg border px-3 py-2 text-left transition-colors ${selected ? 'border-brand/40 bg-brand/8' : 'border-[var(--hairline)] hover:bg-surface-2/45'}`}
                    onClick={() => setSelectedMethod(item.payment_method)}
                  >
                    <span className="grid size-7 shrink-0 place-items-center rounded-md bg-surface-2 text-muted-foreground">{paymentMethodIcon(item)}</span>
                    <span className="min-w-0 text-xs font-medium">{t(`subscriptions.catalog.paymentMethods.${item.payment_method}`)}</span>
                  </button>
                )
              })}
            </div>
          ) : null}
          <Button type="button" size="sm" className="w-fit" disabled={method === undefined || busy} onClick={() => void createOrder()}>
            {busy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <CreditCard aria-hidden="true" />}
            {t(busy ? 'subscriptions.catalog.paymentCreating' : 'subscriptions.catalog.orderCreate')}
          </Button>
        </div>
      ) : null}

      {order !== undefined ? (
        <div className="grid gap-3 border-t border-[var(--hairline)] pt-3">
          <div className="flex flex-wrap items-center gap-2 text-xs">
            <Badge className="bg-surface-2 font-mono text-muted-foreground">{order.status}</Badge>
            <span className="text-muted-foreground">{t('subscriptions.catalog.orderId')}</span>
            <span className="font-mono text-muted-foreground">{order.order_id}</span>
          </div>

          {order.status === 'paid' && active ? (
            <div className="flex flex-wrap items-center gap-2 text-xs text-success">
              <CheckCircle2 className="size-3.5" aria-hidden="true" />
              <span>{t('subscriptions.catalog.paymentEffective')}</span>
              <Button type="button" size="sm" variant="secondary" onClick={clearAttempt}>{t('subscriptions.actions.done')}</Button>
            </div>
          ) : order.status === 'paid' ? (
            <div className="flex flex-wrap items-center gap-2 text-xs text-warning-foreground">
              <RefreshCw className={subscriptionsFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
              <span>{t('subscriptions.catalog.paymentPaid')}</span>
              <Button type="button" size="sm" variant="secondary" disabled={orderQuery.isFetching || subscriptionsFetching} onClick={() => void refreshOrder()}>
                <RefreshCw aria-hidden="true" />
                {t('subscriptions.catalog.refreshSubscription')}
              </Button>
            </div>
          ) : phase !== undefined ? (
            <div className="flex flex-wrap items-center gap-2 text-xs text-warning-foreground">
              <RefreshCw className={orderQuery.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
              <span>{t(`subscriptions.catalog.payment${phase === 'succeeded' ? 'Confirmed' : 'Processing'}`)}</span>
              <Button type="button" size="sm" variant="secondary" disabled={orderQuery.isFetching} onClick={() => void refreshOrder()}>
                <RefreshCw aria-hidden="true" />
                {t('subscriptions.catalog.refreshPayment')}
              </Button>
            </div>
          ) : payment?.client_secret && stripe ? (
            <Elements stripe={stripe} options={{ clientSecret: payment.client_secret, appearance: stripeTopupAppearance('light'), locale: locale === 'zh-CN' ? 'zh' : 'en' }}>
              <WalletStripePaymentForm amountLabel={amountLabel} returnUrl={`${window.location.origin}/#/console/subscriptions`} onConfirmed={paymentConfirmed} />
            </Elements>
          ) : redirectUrl && method?.qr_enabled && method.provider === 'epay' && method.payment_method !== 'card' ? (
            <EpayQrPayment url={redirectUrl} paymentMethod={method.payment_method} refreshing={orderQuery.isFetching} onRefresh={() => void refreshOrder()} />
          ) : redirectUrl ? (
            <div className="flex flex-wrap items-center gap-2 text-xs">
              <ExternalLink className="size-3.5 text-brand" aria-hidden="true" />
              <span className="text-muted-foreground">{t('subscriptions.catalog.paymentRedirectHint')}</span>
              <Button type="button" size="sm" onClick={() => window.location.assign(redirectUrl)}>
                <ExternalLink aria-hidden="true" />
                {t('subscriptions.catalog.openPayment')}
              </Button>
              <Button type="button" size="sm" variant="secondary" disabled={orderQuery.isFetching} onClick={() => void refreshOrder()}>
                <RefreshCw aria-hidden="true" />
                {t('subscriptions.catalog.refreshPayment')}
              </Button>
            </div>
          ) : paymentMutation.isError || createMutation.isError ? (
            <div role="alert" className="flex flex-wrap items-center gap-2 text-xs text-destructive">
              <AlertTriangle className="size-3.5" aria-hidden="true" />
              <span>{t(`subscriptions.errors.${paymentErrorKey}`)}</span>
              <Button type="button" size="sm" variant="secondary" disabled={busy} onClick={() => void (attempt?.orderId ? resumePayment() : createOrder())}>
                <RefreshCw aria-hidden="true" />
                {t('subscriptions.catalog.retryPayment')}
              </Button>
            </div>
          ) : busy ? (
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" />
              {t('subscriptions.catalog.paymentCreating')}
            </div>
          ) : ['failed', 'canceled', 'expired'].includes(order.status) ? (
            <div className="flex flex-wrap items-center gap-2 text-xs text-destructive">
              <AlertTriangle className="size-3.5" aria-hidden="true" />
              <span>{t(`subscriptions.catalog.payment${order.status[0].toUpperCase()}${order.status.slice(1)}`)}</span>
              <Button type="button" size="sm" variant="secondary" onClick={clearAttempt}>
                <RotateCcw aria-hidden="true" />
                {t('subscriptions.catalog.newOrder')}
              </Button>
            </div>
          ) : (
            <div className="flex flex-wrap items-center gap-2 text-xs">
              <span className="text-muted-foreground">{t('subscriptions.catalog.paymentResumeHint')}</span>
              <Button type="button" size="sm" onClick={() => void resumePayment()} disabled={method === undefined || busy}>
                <RotateCcw aria-hidden="true" />
                {t('subscriptions.catalog.resumePayment')}
              </Button>
            </div>
          )}
          {attempt?.orderId === undefined ? <p className="text-[0.6875rem] text-muted-foreground">{t('subscriptions.catalog.orderRecoveryPending')}</p> : null}
        </div>
      ) : null}

      {attempt !== undefined && order === undefined && orderQuery.isError ? (
        <div role="alert" className="flex flex-wrap items-center gap-2 text-xs text-destructive">
          <AlertTriangle className="size-3.5" aria-hidden="true" />
          <span>{t('subscriptions.errors.paymentOrderLoad')}</span>
          <Button type="button" size="sm" variant="secondary" onClick={() => void orderQuery.refetch()}>
            <RefreshCw aria-hidden="true" />
            {t('subscriptions.actions.retry')}
          </Button>
        </div>
      ) : null}
    </article>
  )
}

function paymentMethodIcon(method: Pick<UserTopupMethod, 'payment_method'>) {
  if (method.payment_method === 'card') return <CreditCard className="size-3.5" aria-hidden="true" />
  if (method.payment_method === 'alipay') return <Landmark className="size-3.5" aria-hidden="true" />
  return <Smartphone className="size-3.5" aria-hidden="true" />
}

function subscriptionAttemptStorageKey(planId: string) {
  return `anyflows.subscription.payment-attempt:${planId}`
}

function readStoredAttempt(plan: Pick<SubscriptionCatalogPlan, 'plan_id' | 'price_provider'>): StoredSubscriptionPaymentAttempt | undefined {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(subscriptionAttemptStorageKey(plan.plan_id)) ?? 'null')
    if (!value || typeof value !== 'object') return undefined
    const candidate = value as Partial<StoredSubscriptionPaymentAttempt>
    if (candidate.planId !== plan.plan_id || candidate.provider !== plan.price_provider || typeof candidate.idempotencyKey !== 'string') return undefined
    if (!['card', 'alipay', 'wxpay'].includes(candidate.paymentMethod ?? '')) return undefined
    return candidate as StoredSubscriptionPaymentAttempt
  } catch {
    return undefined
  }
}

function clearStoredAttempt(planId: string) {
  try {
    sessionStorage.removeItem(subscriptionAttemptStorageKey(planId))
  } catch {
    // 存储清理失败不影响服务端订单状态。
  }
}

/** 仅允许服务端返回的 HTTP(S) 地址触发离站支付跳转。 */
function safePaymentRedirectUrl(value: string) {
  try {
    const url = new URL(value)
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.toString() : undefined
  } catch {
    return undefined
  }
}
