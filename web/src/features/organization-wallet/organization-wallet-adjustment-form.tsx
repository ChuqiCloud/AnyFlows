import { zodResolver } from '@hookform/resolvers/zod'
import { ArrowDownToLine, ArrowUpFromLine, LoaderCircle } from 'lucide-react'
import { useRef, useState } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import {
  buildWalletAdjustmentSchema,
  createWalletAdjustmentAttempt,
  defaultWalletAdjustmentValues,
  previewWalletBalance,
  toWalletAdjustmentRequest,
  walletAdjustmentAttemptForValues,
  type WalletAdjustmentDirection,
  type WalletAdjustmentValues,
} from '@/features/users/wallet-form-model'
import { cn } from '@/lib/utils'
import { organizationWalletErrorCode, useAdjustOrganizationWallet } from './organization-wallet-api'

/** 企业调账沿用个人钱包的事件键与精确整数校验，但请求主体固定为当前企业。 */
export function OrganizationWalletAdjustmentForm({ organizationId, balance }: { organizationId: number; balance: number }) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const mutation = useAdjustOrganizationWallet(organizationId)
  const initialValues = defaultWalletAdjustmentValues()
  const attemptRef = useRef(createWalletAdjustmentAttempt(initialValues))
  const [lastResult, setLastResult] = useState<{ replayed: boolean }>()
  const form = useForm<WalletAdjustmentValues>({
    defaultValues: initialValues,
    resolver: zodResolver(buildWalletAdjustmentSchema({
      invalidAmount: t('organizationConsole.wallet.adjustment.validation.amount'),
      invalidReason: t('organizationConsole.wallet.adjustment.validation.reason'),
    })),
  })
  const direction = form.watch('direction')
  const amount = form.watch('amount')
  const preview = previewWalletBalance(balance, direction, amount)
  const markChanged = (values: WalletAdjustmentValues) => {
    attemptRef.current = walletAdjustmentAttemptForValues(attemptRef.current, values)
    setLastResult(undefined)
    mutation.reset()
  }
  const setDirection = (next: WalletAdjustmentDirection) => {
    if (next === form.getValues('direction')) return
    const values = { ...form.getValues(), direction: next }
    form.setValue('direction', next, { shouldDirty: true, shouldValidate: form.formState.isSubmitted })
    markChanged(values)
  }
  const amountField = form.register('amount')
  const reasonField = form.register('reason')
  const submit = form.handleSubmit(async (values) => {
    try {
      const result = await mutation.mutateAsync(toWalletAdjustmentRequest(values, attemptRef.current.eventId))
      setLastResult({ replayed: result.replayed })
      const resetValues = defaultWalletAdjustmentValues()
      form.reset(resetValues)
      attemptRef.current = createWalletAdjustmentAttempt(resetValues)
    } catch {
      // 请求失败时保留原事件键和业务事实，避免重试产生重复账本。
    }
  })
  const submitDisabled = mutation.isPending || preview.status === 'insufficient' || preview.status === 'overflow'
  return (
    <form className="rounded-xl border border-[var(--hairline)] bg-surface-1/35 p-4" onSubmit={submit} noValidate>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div><h3 className="text-sm font-semibold">{t('organizationConsole.wallet.adjustment.title')}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('organizationConsole.wallet.adjustment.subtitle')}</p></div>
        <span className="text-sm font-semibold tabular-nums">{formatQuota(balance)}</span>
      </div>
      <div className="mt-4 grid grid-cols-2 gap-1 rounded-lg bg-surface-2 p-1" role="group" aria-label={t('organizationConsole.wallet.adjustment.direction')}>
        {(['increase', 'decrease'] as const).map((value) => <Button key={value} type="button" size="sm" variant="ghost" className={cn('justify-center', direction === value && 'bg-background shadow-xs hover:bg-background')} aria-pressed={direction === value} disabled={mutation.isPending} onClick={() => setDirection(value)}>{value === 'increase' ? <ArrowUpFromLine aria-hidden="true" /> : <ArrowDownToLine aria-hidden="true" />}{t(`organizationConsole.wallet.adjustment.${value}`)}</Button>)}
      </div>
      <div className="mt-4 grid gap-4 sm:grid-cols-2">
        <div><label htmlFor="organization-wallet-amount" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.wallet.adjustment.amount')}</label><Input id="organization-wallet-amount" type="number" min={1} max={Number.MAX_SAFE_INTEGER} step={1} inputMode="numeric" aria-invalid={!!form.formState.errors.amount} disabled={mutation.isPending} {...amountField} onChange={(event) => { amountField.onChange(event); markChanged({ ...form.getValues(), amount: event.target.value }) }} />{form.formState.errors.amount?.message ? <p className="mt-1 text-xs text-destructive">{form.formState.errors.amount.message}</p> : null}</div>
        <div className="rounded-lg border border-[var(--hairline)] bg-surface-2/55 px-3 py-2"><div className="text-[0.6875rem] text-muted-foreground">{t('organizationConsole.wallet.adjustment.preview')}</div><div className={cn('mt-1 text-sm font-semibold tabular-nums', ['insufficient', 'overflow'].includes(preview.status) && 'text-destructive')}>{preview.status === 'ready' && preview.balance !== undefined ? formatQuota(preview.balance) : t(`organizationConsole.wallet.adjustment.previewState.${preview.status}`)}</div></div>
      </div>
      <div className="mt-4"><label htmlFor="organization-wallet-reason" className="mb-1.5 block text-xs font-medium">{t('organizationConsole.wallet.adjustment.reason')}</label><Textarea id="organization-wallet-reason" rows={3} maxLength={500} aria-invalid={!!form.formState.errors.reason} disabled={mutation.isPending} {...reasonField} onChange={(event) => { reasonField.onChange(event); markChanged({ ...form.getValues(), reason: event.target.value }) }} />{form.formState.errors.reason?.message ? <p className="mt-1 text-xs text-destructive">{form.formState.errors.reason.message}</p> : null}</div>
      {lastResult ? <p role="status" className="mt-3 rounded-lg bg-success/8 px-3 py-2 text-xs text-success">{t(lastResult.replayed ? 'organizationConsole.wallet.adjustment.replayed' : 'organizationConsole.wallet.adjustment.applied')}</p> : null}
      {mutation.isError ? <p role="alert" className="mt-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">{t(`organizationConsole.wallet.errors.${organizationWalletErrorCode(mutation.error) ?? 'unknown'}`, { defaultValue: t('organizationConsole.errors.mutation') })}</p> : null}
      <div className="mt-4 flex justify-end"><Button type="submit" disabled={submitDisabled}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('organizationConsole.wallet.adjustment.submit')}</Button></div>
    </form>
  )
}
