import { Elements } from '@stripe/react-stripe-js'
import { loadStripe, type PaymentIntent } from '@stripe/stripe-js'
import {
  AlertTriangle,
  CheckCircle2,
  CreditCard,
  ExternalLink,
  Landmark,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  Smartphone,
} from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { PaymentMethod, UserTopupMethod } from '@/features/payment-settings/payment-settings-types'
import { WalletStripePaymentForm } from '@/features/wallet/wallet-stripe-payment-form'
import { stripeTopupAppearance } from '@/features/wallet/wallet-topup-runtime'
import type { OrganizationPlan, OrganizationPlanOrder } from '@/lib/api/generated/types.gen'

import {
  createOrganizationPlanOrderIdempotencyKey,
  organizationPlanOrderErrorCode,
  useCreateOrganizationPlanOrder,
  useOrganizationPlanOrder,
  useSubmitOrganizationPlanOrderPayment,
} from './organization-plan-api'

type StoredOrganizationPlanPaymentAttempt = {
  idempotencyKey: string
  organizationId: number
  orderId?: string
  paymentMethod: PaymentMethod
  planId: string
  provider: string
}

type PaymentPhase = 'processing' | 'succeeded'

type OrganizationPlanPurchaseProps = {
  organizationId: number
  plan: OrganizationPlan
  methods: UserTopupMethod[] | undefined
  methodsLoading: boolean
  methodsUnavailable: boolean
  canPurchase: boolean
}

/** 管理企业授权订单的创建、同单支付恢复和服务端状态刷新。 */
export function OrganizationPlanPurchase({
  organizationId,
  plan,
  methods,
  methodsLoading,
  methodsUnavailable,
  canPurchase,
}: OrganizationPlanPurchaseProps) {
  const { i18n, t } = useTranslation()
  const createMutation = useCreateOrganizationPlanOrder(organizationId)
  const paymentMutation = useSubmitOrganizationPlanOrderPayment(organizationId)
  const availableMethods = useMemo(() => methods?.filter((method) => (
    method.provider === plan.price_provider
    && method.currency === plan.price_currency
    && plan.price_amount_minor >= method.min_amount_minor
    && plan.price_amount_minor <= method.max_amount_minor
  )) ?? [], [methods, plan.price_amount_minor, plan.price_currency, plan.price_provider])
  const [attempt, setAttempt] = useState(() => readStoredAttempt(organizationId, plan))
  const [selectedMethod, setSelectedMethod] = useState<PaymentMethod | undefined>(() => attempt?.paymentMethod)
  const [order, setOrder] = useState<OrganizationPlanOrder>()
  const [payment, setPayment] = useState<{
    client_secret: string | null
    payment_intent_id: string | null
    redirect_url: string | null
  }>()
  const [phase, setPhase] = useState<PaymentPhase>()
  const orderQuery = useOrganizationPlanOrder(
    organizationId,
    canPurchase ? attempt?.orderId : undefined,
  )
  const method = attempt?.orderId !== undefined
    ? availableMethods.find((item) => item.payment_method === attempt.paymentMethod)
    : availableMethods.find((item) => item.payment_method === selectedMethod) ?? availableMethods[0]
  const stripe = useMemo(() => method?.provider === 'stripe' && method.publishable_key
    ? loadStripe(method.publishable_key)
    : null, [method])
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const amountLabel = useMemo(() => new Intl.NumberFormat(locale, {
    currency: plan.price_currency,
    style: 'currency',
  }).format(plan.price_amount_minor / 100), [locale, plan.price_amount_minor, plan.price_currency])
  const busy = createMutation.isPending || paymentMutation.isPending
  const errorCode = organizationPlanOrderErrorCode(paymentMutation.error)
    ?? organizationPlanOrderErrorCode(createMutation.error)

  useEffect(() => {
    if (orderQuery.data !== undefined) setOrder(orderQuery.data)
  }, [orderQuery.data])

  useEffect(() => {
    if (availableMethods.length === 0 || attempt?.orderId !== undefined) return
    setSelectedMethod((current) => availableMethods.some((item) => item.payment_method === current)
      ? current
      : availableMethods[0].payment_method)
  }, [attempt?.orderId, availableMethods])

  const persistAttempt = (next: StoredOrganizationPlanPaymentAttempt) => {
    setAttempt(next)
    try {
      sessionStorage.setItem(
        organizationPlanAttemptStorageKey(organizationId, plan.plan_id),
        JSON.stringify(next),
      )
    } catch {
      // 存储不可用时仍保留当前页面内的同单支付恢复能力。
    }
  }

  const clearAttempt = () => {
    clearStoredAttempt(organizationId, plan.plan_id)
    setAttempt(undefined)
    setOrder(undefined)
    setPayment(undefined)
    setPhase(undefined)
    createMutation.reset()
    paymentMutation.reset()
  }

  const submitPayment = async (
    orderId: string,
    paymentMethod: PaymentMethod,
    idempotencyKey = attempt?.idempotencyKey ?? createOrganizationPlanOrderIdempotencyKey(),
  ) => {
    const result = await paymentMutation.mutateAsync({
      body: { payment_method: paymentMethod },
      orderId,
    })
    setOrder(result.order)
    setPayment(result.payment)
    setPhase(undefined)
    persistAttempt({
      idempotencyKey,
      organizationId,
      orderId,
      paymentMethod,
      planId: plan.plan_id,
      provider: plan.price_provider,
    })
  }

  const createOrder = async () => {
    if (method === undefined || busy) return
    const idempotencyKey = attempt?.idempotencyKey ?? createOrganizationPlanOrderIdempotencyKey()
    persistAttempt({
      idempotencyKey,
      organizationId,
      paymentMethod: method.payment_method,
      planId: plan.plan_id,
      provider: plan.price_provider,
    })
    const nextOrder = await createMutation.mutateAsync({
      idempotency_key: idempotencyKey,
      plan_id: plan.plan_id,
    })
    setOrder(nextOrder)
    persistAttempt({
      idempotencyKey,
      organizationId,
      orderId: nextOrder.order_id,
      paymentMethod: method.payment_method,
      planId: plan.plan_id,
      provider: plan.price_provider,
    })
    await submitPayment(nextOrder.order_id, method.payment_method, idempotencyKey)
  }

  const resumePayment = async () => {
    if (attempt?.orderId === undefined || method === undefined || busy) return
    await submitPayment(attempt.orderId, method.payment_method)
  }

  const refreshOrder = async () => {
    const refreshed = await orderQuery.refetch()
    if (refreshed.data !== undefined) setOrder(refreshed.data)
  }

  const paymentConfirmed = async (status: PaymentIntent.Status) => {
    setPhase(status === 'succeeded' ? 'succeeded' : 'processing')
    await refreshOrder()
  }

  const paymentErrorKey = errorCode === 'organization_plan_order_conflict'
    ? 'orderConflict'
    : errorCode === 'organization_plan_order_outcome_unknown'
      ? 'paymentOutcomeUnknown'
      : errorCode === 'organization_plan_order_unavailable' || errorCode === 'topup_unavailable'
        ? 'paymentUnavailable'
        : errorCode === 'topup_provider_rejected'
          ? 'paymentRejected'
          : errorCode === 'forbidden'
            ? 'permission'
            : 'paymentSubmit'
  const redirectUrl = payment?.redirect_url ? safePaymentRedirectUrl(payment.redirect_url) : undefined

  if (!canPurchase) {
    return (
      <p className="border-t border-[var(--hairline)] pt-3 text-xs leading-5 text-muted-foreground">
        {t('organizationConsole.overview.planCatalog.permissionRequired')}
      </p>
    )
  }

  if (attempt?.orderId !== undefined && order === undefined) {
    return orderQuery.isError ? (
      <PaymentError
        message={t('organizationConsole.overview.planCatalog.errors.orderLoad')}
        busy={orderQuery.isFetching}
        retry={() => void orderQuery.refetch()}
        reset={clearAttempt}
      />
    ) : (
      <div className="flex items-center gap-2 border-t border-[var(--hairline)] pt-3 text-xs text-muted-foreground">
        <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" />
        {t('organizationConsole.overview.planCatalog.orderLoading')}
      </div>
    )
  }

  if (order === undefined) {
    return (
      <div className="grid gap-3 border-t border-[var(--hairline)] pt-3">
        <PaymentMethodPicker
          methods={availableMethods}
          method={method}
          loading={methodsLoading}
          unavailable={methodsUnavailable}
          onSelect={setSelectedMethod}
        />
        <Button type="button" size="sm" className="w-fit" disabled={method === undefined || busy} onClick={() => void createOrder()}>
          {busy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <CreditCard aria-hidden="true" />}
          {t(busy ? 'organizationConsole.overview.planCatalog.paymentCreating' : 'organizationConsole.overview.planCatalog.orderCreate')}
        </Button>
        {createMutation.isError ? (
          <PaymentError
            message={t('organizationConsole.overview.planCatalog.errors.' + paymentErrorKey)}
            busy={busy}
            retry={() => void createOrder()}
          />
        ) : null}
      </div>
    )
  }

  return (
    <div className="grid gap-3 border-t border-[var(--hairline)] pt-3">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <Badge className="bg-surface-2 font-mono text-muted-foreground">
          {t('organizationConsole.overview.planCatalog.status.' + order.status)}
        </Badge>
        <span className="text-muted-foreground">{t('organizationConsole.overview.planCatalog.orderId')}</span>
        <span className="font-mono text-muted-foreground">{order.order_id.slice(0, 12)}</span>
      </div>
      {order.status === 'paid' ? (
        <PaymentStatus
          icon={CheckCircle2}
          message={t('organizationConsole.overview.planCatalog.paymentPaid')}
          refreshing={orderQuery.isFetching}
          refresh={() => void refreshOrder()}
          tone="text-success"
        />
      ) : phase !== undefined ? (
        <PaymentStatus
          icon={RefreshCw}
          message={t(phase === 'succeeded'
            ? 'organizationConsole.overview.planCatalog.paymentConfirmed'
            : 'organizationConsole.overview.planCatalog.paymentProcessing')}
          refreshing={orderQuery.isFetching}
          refresh={() => void refreshOrder()}
          tone="text-warning-foreground"
        />
      ) : payment?.client_secret && stripe ? (
        <Elements stripe={stripe} options={{ clientSecret: payment.client_secret, appearance: stripeTopupAppearance('light'), locale: locale === 'zh-CN' ? 'zh' : 'en' }}>
          <WalletStripePaymentForm
            amountLabel={amountLabel}
            returnUrl={window.location.origin + '/#/console/organization'}
            onConfirmed={paymentConfirmed}
          />
        </Elements>
      ) : redirectUrl ? (
        <RedirectPayment
          url={redirectUrl}
          refreshing={orderQuery.isFetching}
          refresh={() => void refreshOrder()}
        />
      ) : paymentMutation.isError ? (
        <PaymentError
          message={t('organizationConsole.overview.planCatalog.errors.' + paymentErrorKey)}
          busy={busy}
          retry={() => void resumePayment()}
        />
      ) : busy ? (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" />
          {t('organizationConsole.overview.planCatalog.paymentCreating')}
        </div>
      ) : ['failed', 'canceled', 'expired'].includes(order.status) ? (
        <div className="flex flex-wrap items-center gap-2 text-xs text-destructive">
          <AlertTriangle className="size-3.5" aria-hidden="true" />
          <span>{t('organizationConsole.overview.planCatalog.paymentTerminal.' + order.status)}</span>
          <Button type="button" size="sm" variant="secondary" onClick={clearAttempt}>
            <RotateCcw aria-hidden="true" />
            {t('organizationConsole.overview.planCatalog.newOrder')}
          </Button>
        </div>
      ) : (
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span className="text-muted-foreground">{t('organizationConsole.overview.planCatalog.paymentResumeHint')}</span>
          <Button type="button" size="sm" disabled={method === undefined || busy} onClick={() => void resumePayment()}>
            <RotateCcw aria-hidden="true" />
            {t('organizationConsole.overview.planCatalog.resumePayment')}
          </Button>
        </div>
      )}
    </div>
  )
}

function PaymentMethodPicker({
  methods,
  method,
  loading,
  unavailable,
  onSelect,
}: {
  methods: UserTopupMethod[]
  method: UserTopupMethod | undefined
  loading: boolean
  unavailable: boolean
  onSelect: (value: PaymentMethod) => void
}) {
  const { t } = useTranslation()
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-xs font-medium">{t('organizationConsole.overview.planCatalog.paymentMethod')}</span>
        {loading ? <span className="text-[0.6875rem] text-muted-foreground">{t('organizationConsole.overview.planCatalog.paymentMethodsLoading')}</span> : null}
      </div>
      {unavailable ? <p role="alert" className="text-xs text-destructive">{t('organizationConsole.overview.planCatalog.paymentMethodsUnavailable')}</p> : null}
      {!loading && !unavailable && methods.length === 0 ? <p className="text-xs text-muted-foreground">{t('organizationConsole.overview.planCatalog.paymentMethodsEmpty')}</p> : null}
      {methods.length > 0 ? (
        <div className="grid gap-2 sm:grid-cols-3" role="radiogroup" aria-label={t('organizationConsole.overview.planCatalog.paymentMethod')}>
          {methods.map((item) => {
            const selected = item.payment_method === method?.payment_method
            return (
              <button
                key={item.payment_method}
                type="button"
                role="radio"
                aria-checked={selected}
                className={selected
                  ? 'flex min-h-12 items-center gap-2 rounded-lg border border-brand/40 bg-brand/8 px-3 py-2 text-left transition-colors'
                  : 'flex min-h-12 items-center gap-2 rounded-lg border border-[var(--hairline)] px-3 py-2 text-left transition-colors hover:bg-surface-2/45'}
                onClick={() => onSelect(item.payment_method)}
              >
                <span className="grid size-7 shrink-0 place-items-center rounded-md bg-surface-2 text-muted-foreground">{paymentMethodIcon(item)}</span>
                <span className="min-w-0 text-xs font-medium">{t('organizationConsole.overview.planCatalog.paymentMethods.' + item.payment_method)}</span>
              </button>
            )
          })}
        </div>
      ) : null}
    </>
  )
}

function PaymentStatus({
  icon: Icon,
  message,
  refreshing,
  refresh,
  tone,
}: {
  icon: typeof CheckCircle2
  message: string
  refreshing: boolean
  refresh: () => void
  tone: string
}) {
  const { t } = useTranslation()
  return (
    <div className={'flex flex-wrap items-center gap-2 text-xs ' + tone}>
      <Icon className={refreshing ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
      <span>{message}</span>
      <Button type="button" size="sm" variant="secondary" disabled={refreshing} onClick={refresh}>
        <RefreshCw aria-hidden="true" />
        {t('organizationConsole.overview.planCatalog.refreshPayment')}
      </Button>
    </div>
  )
}

function RedirectPayment({ url, refreshing, refresh }: { url: string; refreshing: boolean; refresh: () => void }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap items-center gap-2 text-xs">
      <ExternalLink className="size-3.5 text-brand" aria-hidden="true" />
      <span className="text-muted-foreground">{t('organizationConsole.overview.planCatalog.paymentRedirectHint')}</span>
      <Button type="button" size="sm" onClick={() => window.location.assign(url)}>
        <ExternalLink aria-hidden="true" />
        {t('organizationConsole.overview.planCatalog.openPayment')}
      </Button>
      <Button type="button" size="sm" variant="secondary" disabled={refreshing} onClick={refresh}>
        <RefreshCw aria-hidden="true" />
        {t('organizationConsole.overview.planCatalog.refreshPayment')}
      </Button>
    </div>
  )
}

function PaymentError({
  message,
  busy,
  retry,
  reset,
}: {
  message: string
  busy: boolean
  retry: () => void
  reset?: () => void
}) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="flex flex-wrap items-center gap-2 text-xs text-destructive">
      <AlertTriangle className="size-3.5" aria-hidden="true" />
      <span>{message}</span>
      <Button type="button" size="sm" variant="secondary" disabled={busy} onClick={retry}>
        <RefreshCw aria-hidden="true" />
        {t('organizationConsole.actions.retry')}
      </Button>
      {reset ? (
        <Button type="button" size="sm" variant="ghost" disabled={busy} onClick={reset}>
          <RotateCcw aria-hidden="true" />
          {t('organizationConsole.overview.planCatalog.newOrder')}
        </Button>
      ) : null}
    </div>
  )
}

function paymentMethodIcon(method: Pick<UserTopupMethod, 'payment_method'>) {
  if (method.payment_method === 'card') return <CreditCard className="size-3.5" aria-hidden="true" />
  if (method.payment_method === 'alipay') return <Landmark className="size-3.5" aria-hidden="true" />
  return <Smartphone className="size-3.5" aria-hidden="true" />
}

function organizationPlanAttemptStorageKey(organizationId: number, planId: string) {
  return 'anyflows.organization-plan.payment-attempt:' + organizationId + ':' + planId
}

function readStoredAttempt(
  organizationId: number,
  plan: Pick<OrganizationPlan, 'plan_id' | 'price_provider'>,
): StoredOrganizationPlanPaymentAttempt | undefined {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(
      organizationPlanAttemptStorageKey(organizationId, plan.plan_id),
    ) ?? 'null')
    if (!value || typeof value !== 'object') return undefined
    const candidate = value as Partial<StoredOrganizationPlanPaymentAttempt>
    if (
      candidate.organizationId !== organizationId
      || candidate.planId !== plan.plan_id
      || candidate.provider !== plan.price_provider
      || typeof candidate.idempotencyKey !== 'string'
      || !/^[0-9a-f]{32}$/.test(candidate.idempotencyKey)
      || !['card', 'alipay', 'wxpay'].includes(candidate.paymentMethod ?? '')
    ) return undefined
    return candidate as StoredOrganizationPlanPaymentAttempt
  } catch {
    return undefined
  }
}

function clearStoredAttempt(organizationId: number, planId: string) {
  try {
    sessionStorage.removeItem(organizationPlanAttemptStorageKey(organizationId, planId))
  } catch {
    // 存储清理失败不影响服务端订单或授权状态。
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
