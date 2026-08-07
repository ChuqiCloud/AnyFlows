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
  LoaderCircle,
  RefreshCw,
  RotateCcw,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import type { UserTopupOrder } from '@/lib/api/generated/types.gen'
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
import {
  apiErrorCode,
  clearStoredTopupAttempt,
  publicTopupAmountBounds,
  readStoredTopupAttempt,
  storeTopupAttempt,
  stripeTopupAppearance,
  stripeTopupReturnUrl,
  topupErrorKey,
  useRootTheme,
  useStripeRedirectStatus,
  type TopupConfirmationPhase,
} from './wallet-topup-runtime'
import { TopupLoading, TopupState, TopupUnavailable } from './wallet-topup-states'

const presetAmounts = [500, 1_000, 2_500, 5_000] as const

/** 组合金额、幂等订单恢复与 Stripe Payment Element 的本人充值流程。 */
export function WalletTopupPanel() {
  const { i18n, t } = useTranslation()
  const queryClient = useQueryClient()
  const configurationQuery = useUserTopupConfiguration()
  const orderMutation = useCreateUserTopupOrder()
  const createOrRecoverOrder = orderMutation.mutateAsync
  const resetOrderMutation = orderMutation.reset
  const [restoredAttempt] = useState(readStoredTopupAttempt)
  const [attempt, setAttempt] = useState<TopupAttempt | undefined>(restoredAttempt)
  const [amountInput, setAmountInput] = useState(() => (
    restoredAttempt ? topupAmountInput(restoredAttempt.amountMinor) : '10.00'
  ))
  const [order, setOrder] = useState<UserTopupOrder>()
  const [confirmation, setConfirmation] = useState<TopupConfirmationPhase>()
  const redirectStatus = useStripeRedirectStatus()
  const redirectRecoveryStarted = useRef(false)
  const theme = useRootTheme()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const configuration = configurationQuery.data
  const bounds = configuration
    ? { min: configuration.min_amount_minor, max: configuration.max_amount_minor }
    : publicTopupAmountBounds
  const amountMinor = topupAmountMinor(amountInput, bounds)
  const moneyFormat = useMemo(() => new Intl.NumberFormat(locale, {
    currency: configuration?.currency ?? 'USD',
    style: 'currency',
  }), [configuration?.currency, locale])
  const quotaFormat = useMemo(() => new Intl.NumberFormat(locale), [locale])
  const stripe = useMemo(() => (
    configuration?.publishable_key
      ? loadStripe(configuration.publishable_key)
      : null
  ), [configuration?.publishable_key])

  const applyOrder = useCallback(async (nextOrder: UserTopupOrder) => {
    setOrder(nextOrder)
    if (nextOrder.status !== 'paid') return

    clearStoredTopupAttempt()
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: userWalletSummaryQueryKey }),
      queryClient.invalidateQueries({ queryKey: userWalletEntriesQueryKey }),
    ])
  }, [queryClient])

  const submitAttempt = useCallback(async (currentAttempt: TopupAttempt) => {
    resetOrderMutation()
    const nextOrder = await createOrRecoverOrder({
      amount_minor: currentAttempt.amountMinor,
      idempotency_key: currentAttempt.idempotencyKey,
    })
    await applyOrder(nextOrder)
  }, [applyOrder, createOrRecoverOrder, resetOrderMutation])

  useEffect(() => {
    if (
      redirectRecoveryStarted.current
      || !redirectStatus
      || !configuration
      || !attempt
    ) return

    redirectRecoveryStarted.current = true
    setConfirmation(redirectStatus === 'failed' ? undefined : redirectStatus)
    void submitAttempt(attempt).catch(() => undefined)
  }, [attempt, configuration, redirectStatus, submitAttempt])

  const startTopup = () => {
    if (amountMinor === undefined || !configuration) return
    const nextAttempt = createTopupAttempt(amountMinor)
    storeTopupAttempt(nextAttempt)
    setAttempt(nextAttempt)
    setOrder(undefined)
    setConfirmation(undefined)
    void submitAttempt(nextAttempt).catch(() => undefined)
  }

  const retryAttempt = () => {
    if (!attempt) return
    void submitAttempt(attempt).catch(() => undefined)
  }

  const resetAttempt = () => {
    clearStoredTopupAttempt()
    setAttempt(undefined)
    setOrder(undefined)
    setConfirmation(undefined)
    orderMutation.reset()
  }

  const paymentConfirmed = async (status: PaymentIntent.Status) => {
    if (!attempt) return
    setConfirmation(
      status === 'succeeded'
        ? 'succeeded'
        : status === 'processing' || status === 'requires_capture'
          ? 'processing'
          : undefined,
    )
    await submitAttempt(attempt)
  }

  if (configurationQuery.isPending) return <TopupLoading />
  if (configurationQuery.isError || !configuration) {
    return (
      <TopupUnavailable
        onRetry={() => void configurationQuery.refetch()}
        retryable={apiErrorCode(configurationQuery.error) !== 'topup_unavailable'}
      />
    )
  }

  const amountLocked = attempt !== undefined
  const activePayment = order?.status === 'pending' && !confirmation
    ? order.payment ?? undefined
    : undefined
  const appearance = stripeTopupAppearance(theme)
  const amountLabel = moneyFormat.format((order?.amount_minor ?? attempt?.amountMinor ?? 0) / 100)

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-0">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] bg-surface-1/45 px-4 py-3.5">
          <div className="flex min-w-0 items-center gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand">
              <CircleDollarSign className="size-4" aria-hidden="true" />
            </span>
            <div className="min-w-0">
              <h3 className="text-sm font-semibold">{t('wallet.topup.title')}</h3>
              <p className="mt-0.5 text-xs text-muted-foreground">{t('wallet.topup.description')}</p>
            </div>
          </div>
          <Badge className="gap-1.5 bg-surface-2 text-muted-foreground">
            <CreditCard className="size-3" aria-hidden="true" />
            Stripe · {configuration.currency}
          </Badge>
        </div>

        <div className="grid lg:grid-cols-[minmax(0,0.82fr)_minmax(20rem,1.18fr)]">
          <section className="border-b border-[var(--hairline)] p-4 lg:border-r lg:border-b-0">
            <div className="flex items-center justify-between gap-3">
              <Label htmlFor="wallet-topup-amount">{t('wallet.topup.amount.label')}</Label>
              {amountLocked ? (
                <Button type="button" size="sm" variant="ghost" onClick={resetAttempt}>
                  <RotateCcw aria-hidden="true" />
                  {t('wallet.topup.actions.changeAmount')}
                </Button>
              ) : null}
            </div>
            <div className="relative mt-2">
              <span className="pointer-events-none absolute inset-y-0 left-3 flex items-center text-sm text-muted-foreground">$</span>
              <Input
                id="wallet-topup-amount"
                className="pl-7 text-base font-semibold tabular-nums"
                inputMode="decimal"
                autoComplete="off"
                disabled={amountLocked}
                value={amountInput}
                onChange={(event) => setAmountInput(event.target.value)}
              />
            </div>
            <div className="mt-2 grid grid-cols-4 gap-1 rounded-lg bg-surface-2 p-1" role="group" aria-label={t('wallet.topup.amount.presets')}>
              {presetAmounts.map((value) => (
                <Button
                  key={value}
                  type="button"
                  size="sm"
                  variant={amountMinor === value ? 'secondary' : 'ghost'}
                  disabled={amountLocked || value < bounds.min || value > bounds.max}
                  className="px-1 tabular-nums"
                  onClick={() => setAmountInput(topupAmountInput(value))}
                >
                  {moneyFormat.format(value / 100)}
                </Button>
              ))}
            </div>
            {amountMinor === undefined ? (
              <p role="alert" className="mt-2 text-xs leading-5 text-destructive">
                {t('wallet.topup.amount.validation', {
                  min: moneyFormat.format(bounds.min / 100),
                  max: moneyFormat.format(bounds.max / 100),
                })}
              </p>
            ) : (
              <p className="mt-2 text-[0.6875rem] leading-5 text-muted-foreground">
                {t('wallet.topup.amount.hint', {
                  min: moneyFormat.format(bounds.min / 100),
                  max: moneyFormat.format(bounds.max / 100),
                })}
              </p>
            )}
            {order ? (
              <div className="mt-4 flex items-center justify-between gap-3 border-t border-[var(--hairline)] pt-3 text-xs">
                <span className="text-muted-foreground">{t('wallet.topup.amount.quota')}</span>
                <span className="font-semibold tabular-nums">{quotaFormat.format(order.quota_amount)}</span>
              </div>
            ) : null}
          </section>

          <section className="min-w-0 p-4">
            {order?.status === 'paid' ? (
              <TopupState
                icon={CheckCircle2}
                tone="success"
                title={t('wallet.topup.states.paidTitle')}
                body={t('wallet.topup.states.paidBody', { amount: quotaFormat.format(order.quota_amount) })}
                action={<Button type="button" size="sm" variant="secondary" onClick={resetAttempt}>{t('wallet.topup.actions.done')}</Button>}
              />
            ) : confirmation === 'succeeded' || confirmation === 'processing' ? (
              <TopupState
                icon={Clock3}
                tone="warning"
                title={t(confirmation === 'succeeded' ? 'wallet.topup.states.confirmedTitle' : 'wallet.topup.states.processingTitle')}
                body={t('wallet.topup.states.confirmedBody')}
                action={<Button type="button" size="sm" variant="secondary" disabled={orderMutation.isPending} onClick={retryAttempt}><RefreshCw className={orderMutation.isPending ? 'animate-spin' : undefined} aria-hidden="true" />{t('wallet.topup.actions.refreshStatus')}</Button>}
              />
            ) : activePayment ? (
              <Elements
                key={activePayment.payment_intent_id}
                stripe={stripe}
                options={{
                  appearance,
                  clientSecret: activePayment.client_secret,
                  locale: locale === 'zh-CN' ? 'zh' : 'en',
                }}
              >
                <WalletStripePaymentForm
                  amountLabel={amountLabel}
                  returnUrl={stripeTopupReturnUrl()}
                  onConfirmed={paymentConfirmed}
                />
              </Elements>
            ) : order && ['failed', 'canceled', 'expired'].includes(order.status) ? (
              <TopupState
                icon={AlertTriangle}
                tone="destructive"
                title={t(`wallet.topup.states.${order.status}Title`)}
                body={t(`wallet.topup.states.${order.status}Body`)}
                action={<Button type="button" size="sm" variant="secondary" onClick={resetAttempt}>{t('wallet.topup.actions.newAttempt')}</Button>}
              />
            ) : orderMutation.isError ? (
              <TopupState
                icon={AlertTriangle}
                tone="destructive"
                title={t('wallet.topup.states.errorTitle')}
                body={t(`wallet.topup.errors.${topupErrorKey(apiErrorCode(orderMutation.error))}`)}
                action={<Button type="button" size="sm" variant="secondary" onClick={retryAttempt}><RefreshCw aria-hidden="true" />{t('wallet.topup.actions.retrySame')}</Button>}
              />
            ) : orderMutation.isPending ? (
              <TopupState
                icon={LoaderCircle}
                tone="brand"
                spinning
                title={t('wallet.topup.states.creatingTitle')}
                body={t('wallet.topup.states.creatingBody')}
              />
            ) : attempt ? (
              <TopupState
                icon={Clock3}
                tone="warning"
                title={t('wallet.topup.states.resumeTitle')}
                body={t('wallet.topup.states.resumeBody', { amount: moneyFormat.format(attempt.amountMinor / 100) })}
                action={<Button type="button" size="sm" onClick={retryAttempt}><RefreshCw aria-hidden="true" />{t('wallet.topup.actions.resume')}</Button>}
              />
            ) : (
              <TopupState
                icon={BadgeDollarSign}
                tone="brand"
                title={t('wallet.topup.states.readyTitle')}
                body={t('wallet.topup.states.readyBody')}
                action={<Button type="button" size="sm" disabled={amountMinor === undefined} onClick={startTopup}><CreditCard aria-hidden="true" />{t('wallet.topup.actions.continue')}</Button>}
              />
            )}
          </section>
        </div>
      </CardContent>
    </Card>
  )
}
