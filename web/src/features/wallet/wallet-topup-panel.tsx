import { Elements } from '@stripe/react-stripe-js'
import { loadStripe, type PaymentIntent } from '@stripe/stripe-js'
import { useQueryClient } from '@tanstack/react-query'
import {
  AlertTriangle,
  BadgeDollarSign,
  CheckCircle2,
  CircleDollarSign,
  Clock3,
  CreditCard,
  ExternalLink,
  Landmark,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  Smartphone,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { cn } from '@/lib/utils'
import type { UserTopupMethod, UserTopupPayment } from '@/features/payment-settings/payment-settings-types'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import {
  createTopupAttempt,
  topupAmountInput,
  topupAmountMinor,
  type TopupAttempt,
} from './topup-form-model'
import {
  userWalletEntriesQueryKey,
  userWalletSummaryQueryKey,
  useCreateUserTopupOrder,
  useUserTopupConfiguration,
} from './wallet-api'
import { WalletStripePaymentForm } from './wallet-stripe-payment-form'
import { EpayQrPayment } from './epay-qr-payment'
import {
  apiErrorCode,
  clearStoredTopupAttempt,
  publicTopupAmountBounds,
  readStoredTopupAttempt,
  storeTopupAttempt,
  stripeTopupAppearance,
  topupErrorKey,
  topupReturnUrl,
  usePaymentRedirectStatus,
  useRootTheme,
  type TopupConfirmationPhase,
} from './wallet-topup-runtime'
import { TopupLoading, TopupState, TopupUnavailable } from './wallet-topup-states'

const presetAmounts = [500, 1_000, 2_500, 5_000] as const

type TopupOrderView = {
  amount_minor: number
  order_id: string
  payment?: UserTopupPayment | null
  quota_amount: number
  status: 'canceled' | 'created' | 'expired' | 'failed' | 'paid' | 'pending'
}

/** 组合支付方式选择、幂等恢复、Stripe 内嵌支付与易支付跳转流程。 */
export function WalletTopupPanel({
  title,
  description,
}: {
  title?: string
  description?: string
} = {}) {
  const { i18n, t } = useTranslation()
  const queryClient = useQueryClient()
  const configurationQuery = useUserTopupConfiguration()
  const orderMutation = useCreateUserTopupOrder()
  const { formatQuota } = useBalanceDisplay()
  const createOrRecoverOrder = orderMutation.mutateAsync
  const resetOrderMutation = orderMutation.reset
  const topupScope = 'personal'
  const returnHash = '#/console/wallet'
  const [restoredAttempt] = useState(() => readStoredTopupAttempt(topupScope))
  const [attempt, setAttempt] = useState<TopupAttempt | undefined>(restoredAttempt)
  const [selectedMethodKey, setSelectedMethodKey] = useState(() => restoredAttempt ? attemptMethodKey(restoredAttempt) : '')
  const [amountInput, setAmountInput] = useState(() => restoredAttempt ? topupAmountInput(restoredAttempt.amountMinor) : '10.00')
  const [order, setOrder] = useState<TopupOrderView>()
  const [confirmation, setConfirmation] = useState<TopupConfirmationPhase>()
  const redirectStatus = usePaymentRedirectStatus()
  const redirectRecoveryStarted = useRef(false)
  const theme = useRootTheme()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const methods = useMemo(() => configurationQuery.data?.methods ?? [], [configurationQuery.data?.methods])
  const selectedMethod = methods.find((method) => methodKey(method) === selectedMethodKey) ?? methods[0]
  const bounds = selectedMethod
    ? { min: selectedMethod.min_amount_minor, max: selectedMethod.max_amount_minor }
    : publicTopupAmountBounds
  const amountMinor = topupAmountMinor(amountInput, bounds)
  const moneyFormat = useMemo(() => new Intl.NumberFormat(locale, {
    currency: selectedMethod?.currency ?? 'USD',
    style: 'currency',
  }), [locale, selectedMethod?.currency])
  const stripe = useMemo(() => selectedMethod?.provider === 'stripe' && selectedMethod.publishable_key
    ? loadStripe(selectedMethod.publishable_key)
    : null, [selectedMethod])

  const resetAttempt = useCallback(() => {
    clearStoredTopupAttempt(topupScope)
    setAttempt(undefined)
    setOrder(undefined)
    setConfirmation(undefined)
    resetOrderMutation()
  }, [resetOrderMutation, topupScope])

  useEffect(() => {
    if (methods.length === 0) return
    const restoredAvailable = restoredAttempt && methods.some((method) => methodKey(method) === attemptMethodKey(restoredAttempt))
    if (restoredAvailable) {
      setSelectedMethodKey(attemptMethodKey(restoredAttempt))
      return
    }
    if (restoredAttempt) resetAttempt()
    setSelectedMethodKey((current) => methods.some((method) => methodKey(method) === current) ? current : methodKey(methods[0]))
  }, [methods, resetAttempt, restoredAttempt])

  const applyOrder = useCallback(async (nextOrder: TopupOrderView) => {
    setOrder(nextOrder)
    if (nextOrder.status !== 'paid') return

    clearStoredTopupAttempt(topupScope)
    await Promise.all([
          queryClient.invalidateQueries({ queryKey: userWalletSummaryQueryKey }),
          queryClient.invalidateQueries({ queryKey: userWalletEntriesQueryKey }),
        ])
  }, [queryClient, topupScope])

  const submitAttempt = useCallback(async (currentAttempt: TopupAttempt) => {
    resetOrderMutation()
    const nextOrder = await createOrRecoverOrder({
      amount_minor: currentAttempt.amountMinor,
      idempotency_key: currentAttempt.idempotencyKey,
      payment_method: currentAttempt.paymentMethod,
      provider: currentAttempt.provider,
    })
    await applyOrder(nextOrder)
  }, [applyOrder, createOrRecoverOrder, resetOrderMutation])

  useEffect(() => {
    if (redirectRecoveryStarted.current || !redirectStatus || methods.length === 0 || !attempt) return
    redirectRecoveryStarted.current = true
    setConfirmation(redirectStatus === 'failed' ? undefined : redirectStatus)
    void submitAttempt(attempt).catch(() => undefined)
  }, [attempt, methods.length, redirectStatus, submitAttempt])

  const startTopup = () => {
    if (amountMinor === undefined || !selectedMethod) return
    const nextAttempt = createTopupAttempt(amountMinor, selectedMethod.provider, selectedMethod.payment_method)
    storeTopupAttempt(nextAttempt, topupScope)
    setAttempt(nextAttempt)
    setOrder(undefined)
    setConfirmation(undefined)
    void submitAttempt(nextAttempt).catch(() => undefined)
  }

  const selectMethod = (method: UserTopupMethod) => {
    if (methodKey(method) === selectedMethodKey) return
    resetAttempt()
    setSelectedMethodKey(methodKey(method))
    const currentMinor = topupAmountMinor(amountInput, {
      min: method.min_amount_minor,
      max: method.max_amount_minor,
    })
    if (currentMinor === undefined) setAmountInput(topupAmountInput(method.min_amount_minor))
  }

  const retryAttempt = () => {
    if (!attempt) return
    void submitAttempt(attempt).catch(() => undefined)
  }

  const paymentConfirmed = async (status: PaymentIntent.Status) => {
    if (!attempt) return
    setConfirmation(status === 'succeeded'
      ? 'succeeded'
      : status === 'processing' || status === 'requires_capture'
        ? 'processing'
        : undefined)
    await submitAttempt(attempt)
  }

  if (configurationQuery.isPending) return <TopupLoading />
  if (configurationQuery.isError || methods.length === 0 || !selectedMethod) {
    return <TopupUnavailable onRetry={() => void configurationQuery.refetch()} retryable={configurationQuery.isError && apiErrorCode(configurationQuery.error) !== 'topup_unavailable'} />
  }

  const amountLocked = attempt !== undefined
  const activePayment = order?.status === 'pending' && !confirmation ? order.payment ?? undefined : undefined
  const appearance = stripeTopupAppearance(theme)
  const amountLabel = moneyFormat.format((order?.amount_minor ?? attempt?.amountMinor ?? 0) / 100)
  const redirectUrl = activePayment?.kind === 'redirect'
    ? safePaymentRedirectUrl(activePayment.redirect_url)
    : undefined

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-0">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] bg-surface-1/45 px-4 py-3.5">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand"><CircleDollarSign className="size-4" aria-hidden="true" /></span>
            <div className="min-w-0"><h3 className="text-sm font-semibold">{title ?? t('wallet.topup.title')}</h3><p className="mt-0.5 text-xs text-muted-foreground">{description ?? t('wallet.topup.description')}</p></div>
          </div>
          <Badge className="gap-1.5 bg-surface-2 text-muted-foreground">{paymentMethodIcon(selectedMethod)}{t(`wallet.topup.methods.${selectedMethod.payment_method}`)} · {selectedMethod.currency}</Badge>
        </div>

        <div className="grid lg:grid-cols-[minmax(0,0.9fr)_minmax(20rem,1.1fr)]">
          <section className="border-b border-[var(--hairline)] p-4 lg:border-r lg:border-b-0">
            <div>
              <Label>{t('wallet.topup.methods.label')}</Label>
              <div className="mt-2 grid gap-2 sm:grid-cols-3 lg:grid-cols-1" role="radiogroup" aria-label={t('wallet.topup.methods.label')}>
                {methods.map((method) => {
                  const selected = methodKey(method) === methodKey(selectedMethod)
                  return (
                    <button key={methodKey(method)} type="button" role="radio" aria-checked={selected} className={cn('flex min-h-14 items-center gap-3 rounded-lg border px-3 py-2 text-left transition-colors', selected ? 'border-brand/40 bg-brand/8' : 'border-[var(--hairline)] hover:bg-surface-2/45')} onClick={() => selectMethod(method)}>
                      <span className={cn('grid size-8 shrink-0 place-items-center rounded-lg', selected ? 'bg-brand/12 text-brand' : 'bg-surface-2 text-muted-foreground')}>{paymentMethodIcon(method)}</span>
                      <span className="min-w-0 flex-1"><span className="block text-sm font-medium">{t(`wallet.topup.methods.${method.payment_method}`)}</span><span className="mt-0.5 block text-[0.6875rem] text-muted-foreground">{t(`wallet.topup.providers.${method.provider}`)} · {method.currency}</span></span>
                    </button>
                  )
                })}
              </div>
            </div>

            <div className="mt-4 flex items-center justify-between gap-3"><Label htmlFor="wallet-topup-amount">{t('wallet.topup.amount.label')}</Label>{amountLocked ? <Button type="button" size="sm" variant="ghost" onClick={resetAttempt}><RotateCcw aria-hidden="true" />{t('wallet.topup.actions.changeAmount')}</Button> : null}</div>
            <div className="relative mt-2"><span className="pointer-events-none absolute inset-y-0 left-3 flex items-center text-xs font-medium text-muted-foreground">{selectedMethod.currency}</span><Input id="wallet-topup-amount" className="pl-12 text-base font-semibold tabular-nums" inputMode="decimal" autoComplete="off" disabled={amountLocked} value={amountInput} onChange={(event) => setAmountInput(event.target.value)} /></div>
            <div className="mt-2 grid grid-cols-4 gap-1 rounded-lg bg-surface-2 p-1" role="group" aria-label={t('wallet.topup.amount.presets')}>
              {presetAmounts.map((value) => <Button key={value} type="button" size="sm" variant={amountMinor === value ? 'secondary' : 'ghost'} disabled={amountLocked || value < bounds.min || value > bounds.max} className="px-1 tabular-nums" onClick={() => setAmountInput(topupAmountInput(value))}>{moneyFormat.format(value / 100)}</Button>)}
            </div>
            {amountMinor === undefined ? <p role="alert" className="mt-2 text-xs leading-5 text-destructive">{t('wallet.topup.amount.validation', { min: moneyFormat.format(bounds.min / 100), max: moneyFormat.format(bounds.max / 100) })}</p> : <p className="mt-2 text-[0.6875rem] leading-5 text-muted-foreground">{t('wallet.topup.amount.hint', { min: moneyFormat.format(bounds.min / 100), max: moneyFormat.format(bounds.max / 100) })}</p>}
            {order ? <div className="mt-4 flex items-center justify-between gap-3 border-t border-[var(--hairline)] pt-3 text-xs"><span className="text-muted-foreground">{t('wallet.topup.amount.quota')}</span><span className="font-semibold tabular-nums">{formatQuota(order.quota_amount)}</span></div> : null}
          </section>

          <section className="min-w-0 p-4">
            {order?.status === 'paid' ? <TopupState icon={CheckCircle2} tone="success" title={t('wallet.topup.states.paidTitle')} body={t('wallet.topup.states.paidBody', { amount: formatQuota(order.quota_amount) })} action={<Button type="button" size="sm" variant="secondary" onClick={resetAttempt}>{t('wallet.topup.actions.done')}</Button>} />
              : confirmation === 'succeeded' || confirmation === 'processing' ? <TopupState icon={Clock3} tone="warning" title={t(confirmation === 'succeeded' ? 'wallet.topup.states.confirmedTitle' : 'wallet.topup.states.processingTitle')} body={t('wallet.topup.states.confirmedBody')} action={<Button type="button" size="sm" variant="secondary" disabled={orderMutation.isPending} onClick={retryAttempt}><RefreshCw className={orderMutation.isPending ? 'animate-spin' : undefined} aria-hidden="true" />{t('wallet.topup.actions.refreshStatus')}</Button>} />
              : activePayment?.kind === 'stripe' && stripe ? <Elements key={activePayment.payment_intent_id} stripe={stripe} options={{ appearance, clientSecret: activePayment.client_secret, locale: locale === 'zh-CN' ? 'zh' : 'en' }}><WalletStripePaymentForm amountLabel={amountLabel} returnUrl={topupReturnUrl(returnHash)} onConfirmed={paymentConfirmed} /></Elements>
              : activePayment?.kind === 'redirect' && redirectUrl && selectedMethod.qr_enabled && selectedMethod.provider === 'epay' && selectedMethod.payment_method !== 'card' ? <EpayQrPayment url={redirectUrl} paymentMethod={selectedMethod.payment_method} refreshing={orderMutation.isPending} onRefresh={retryAttempt} />
              : activePayment?.kind === 'redirect' && redirectUrl ? <TopupState icon={ExternalLink} tone="brand" title={t('wallet.topup.states.redirectTitle')} body={t('wallet.topup.states.redirectBody')} action={<Button type="button" size="sm" onClick={() => window.location.assign(redirectUrl)}><ExternalLink aria-hidden="true" />{t('wallet.topup.actions.openPayment')}</Button>} />
              : order && ['failed', 'canceled', 'expired'].includes(order.status) ? <TopupState icon={AlertTriangle} tone="destructive" title={t(`wallet.topup.states.${order.status}Title`)} body={t(`wallet.topup.states.${order.status}Body`)} action={<Button type="button" size="sm" variant="secondary" onClick={resetAttempt}>{t('wallet.topup.actions.newAttempt')}</Button>} />
              : orderMutation.isError ? <TopupState icon={AlertTriangle} tone="destructive" title={t('wallet.topup.states.errorTitle')} body={t(`wallet.topup.errors.${topupErrorKey(apiErrorCode(orderMutation.error))}`)} action={<Button type="button" size="sm" variant="secondary" onClick={retryAttempt}><RefreshCw aria-hidden="true" />{t('wallet.topup.actions.retrySame')}</Button>} />
              : orderMutation.isPending ? <TopupState icon={LoaderCircle} tone="brand" spinning title={t('wallet.topup.states.creatingTitle')} body={t('wallet.topup.states.creatingBody')} />
              : attempt ? <TopupState icon={Clock3} tone="warning" title={t('wallet.topup.states.resumeTitle')} body={t('wallet.topup.states.resumeBody', { amount: moneyFormat.format(attempt.amountMinor / 100) })} action={<Button type="button" size="sm" onClick={retryAttempt}><RefreshCw aria-hidden="true" />{t('wallet.topup.actions.resume')}</Button>} />
              : <TopupState icon={BadgeDollarSign} tone="brand" title={t('wallet.topup.states.readyTitle')} body={t('wallet.topup.states.readyBody')} action={<Button type="button" size="sm" disabled={amountMinor === undefined} onClick={startTopup}><CreditCard aria-hidden="true" />{t('wallet.topup.actions.continue')}</Button>} />}
          </section>
        </div>
      </CardContent>
    </Card>
  )
}

function methodKey(method: Pick<UserTopupMethod, 'payment_method' | 'provider'>) {
  return `${method.provider}:${method.payment_method}`
}

function attemptMethodKey(attempt: Pick<TopupAttempt, 'paymentMethod' | 'provider'>) {
  return `${attempt.provider}:${attempt.paymentMethod}`
}

function paymentMethodIcon(method: Pick<UserTopupMethod, 'payment_method'>) {
  if (method.payment_method === 'card') return <CreditCard className="size-3.5" aria-hidden="true" />
  if (method.payment_method === 'alipay') return <Landmark className="size-3.5" aria-hidden="true" />
  return <Smartphone className="size-3.5" aria-hidden="true" />
}

/** 仅允许服务端签发的 HTTP(S) 地址触发离站支付跳转。 */
function safePaymentRedirectUrl(value: string) {
  try {
    const url = new URL(value)
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.toString() : undefined
  } catch {
    return undefined
  }
}
