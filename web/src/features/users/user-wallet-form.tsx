import { zodResolver } from '@hookform/resolvers/zod'
import { ArrowDownToLine, ArrowUpFromLine, LoaderCircle } from 'lucide-react'
import { useRef, useState } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Textarea } from '@/components/ui/textarea'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { AdminUser } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { userWriteErrorCode, useAdjustAdminWallet } from './user-api'
import { UserFormField } from './user-form-field'
import {
  buildWalletAdjustmentSchema,
  createWalletAdjustmentAttempt,
  defaultWalletAdjustmentValues,
  previewWalletBalance,
  toWalletAdjustmentRequest,
  walletAdjustmentAttemptForValues,
  type WalletAdjustmentDirection,
  type WalletAdjustmentValues,
} from './wallet-form-model'

export function UserWalletForm({ user }: { user: AdminUser }) {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const initialValues = defaultWalletAdjustmentValues()
  const mutation = useAdjustAdminWallet(user.id)
  const attemptRef = useRef(createWalletAdjustmentAttempt(initialValues))
  const [lastResult, setLastResult] = useState<{ replayed: boolean }>()
  const form = useForm<WalletAdjustmentValues>({
    defaultValues: initialValues,
    resolver: zodResolver(buildWalletAdjustmentSchema({
      invalidAmount: t('users.wallet.validation.amount'),
      invalidReason: t('users.wallet.validation.reason'),
    })),
  })
  const direction = form.watch('direction')
  const amount = form.watch('amount')
  const preview = previewWalletBalance(user.quota, direction, amount)
  const markValuesChanged = (values: WalletAdjustmentValues) => {
    attemptRef.current = walletAdjustmentAttemptForValues(attemptRef.current, values)
    setLastResult(undefined)
    mutation.reset()
  }

  const setDirection = (next: WalletAdjustmentDirection) => {
    if (next === form.getValues('direction')) return
    const values = { ...form.getValues(), direction: next }
    form.setValue('direction', next, { shouldDirty: true, shouldValidate: form.formState.isSubmitted })
    markValuesChanged(values)
  }

  const amountField = form.register('amount')
  const reasonField = form.register('reason')
  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const result = await mutation.mutateAsync(toWalletAdjustmentRequest(values, attemptRef.current.eventId))
      setLastResult({ replayed: result.replayed })
      const resetValues = defaultWalletAdjustmentValues()
      form.reset(resetValues)
      attemptRef.current = createWalletAdjustmentAttempt(resetValues)
    } catch {
      // 请求失败时保留原事件键和表单内容，管理员可安全重试同一业务事实。
    }
  })
  const errorCode = userWriteErrorCode(mutation.error)
  const submitDisabled = mutation.isPending || preview.status === 'insufficient' || preview.status === 'overflow'

  return (
    <form className="border-b border-[var(--hairline)] px-4 py-4" onSubmit={onSubmit} noValidate>
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-xs font-semibold">{t('users.wallet.adjustment.title')}</h3>
          <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('users.wallet.adjustment.description')}</p>
        </div>
        <span className="shrink-0 text-sm font-semibold tabular-nums">{formatQuota(user.quota)}</span>
      </div>

      <div className="mt-4 grid grid-cols-2 gap-1 rounded-lg bg-surface-2 p-1" role="group" aria-label={t('users.wallet.fields.direction')}>
        {(['increase', 'decrease'] as const).map((value) => (
          <Button
            key={value}
            type="button"
            size="sm"
            variant="ghost"
            className={cn('justify-center', direction === value && 'bg-background shadow-xs hover:bg-background')}
            disabled={mutation.isPending}
            aria-pressed={direction === value}
            onClick={() => setDirection(value)}
          >
            {value === 'increase' ? <ArrowUpFromLine aria-hidden="true" /> : <ArrowDownToLine aria-hidden="true" />}
            {t(`users.wallet.direction.${value}`)}
          </Button>
        ))}
      </div>

      <div className="mt-4 grid gap-4 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
        <UserFormField id="wallet-amount" label={t('users.wallet.fields.amount')} error={form.formState.errors.amount?.message}>
          <Input
            id="wallet-amount"
            type="number"
            min={1}
            max={Number.MAX_SAFE_INTEGER}
            step={1}
            inputMode="numeric"
            aria-invalid={!!form.formState.errors.amount}
            disabled={mutation.isPending}
            {...amountField}
            onChange={(event) => {
              amountField.onChange(event)
              markValuesChanged({ ...form.getValues(), amount: event.target.value })
            }}
          />
        </UserFormField>
        <div className="rounded-lg border border-[var(--hairline)] bg-surface-2/55 px-3 py-2">
          <div className="text-[0.6875rem] text-muted-foreground">{t('users.wallet.adjustment.preview')}</div>
          <div className={cn(
            'mt-1 text-sm font-semibold tabular-nums',
            preview.status === 'insufficient' && 'text-destructive',
            preview.status === 'overflow' && 'text-destructive',
          )}>
            {preview.status === 'ready' && preview.balance !== undefined
              ? formatQuota(preview.balance)
              : preview.status === 'insufficient'
                ? t('users.wallet.adjustment.insufficientPreview')
                : preview.status === 'overflow'
                  ? t('users.wallet.adjustment.overflowPreview')
                  : t('users.wallet.adjustment.pendingPreview')}
          </div>
        </div>
      </div>

      <div className="mt-4">
        <UserFormField id="wallet-reason" label={t('users.wallet.fields.reason')} hint={t('users.wallet.adjustment.reasonHint')} error={form.formState.errors.reason?.message}>
          <Textarea
            id="wallet-reason"
            rows={3}
            maxLength={500}
            aria-invalid={!!form.formState.errors.reason}
            disabled={mutation.isPending}
            {...reasonField}
            onChange={(event) => {
              reasonField.onChange(event)
              markValuesChanged({ ...form.getValues(), reason: event.target.value })
            }}
          />
        </UserFormField>
      </div>

      {lastResult ? (
        <p role="status" className="mt-3 rounded-lg bg-success/10 px-3 py-2 text-xs text-success">
          {t(lastResult.replayed ? 'users.wallet.success.replayed' : 'users.wallet.success.applied')}
        </p>
      ) : null}
      {mutation.isError ? (
        <p role="alert" className="mt-3 rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {t(`users.wallet.error.${walletErrorTranslationKey(errorCode)}`)}
        </p>
      ) : null}

      <div className="mt-4 flex justify-end">
        <Button type="submit" disabled={submitDisabled}>
          {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t('users.wallet.actions.submit')}
        </Button>
      </div>
    </form>
  )
}

function walletErrorTranslationKey(code: string | undefined) {
  if (code === 'wallet_insufficient_quota') return 'insufficient'
  if (code === 'wallet_overflow') return 'overflow'
  if (code === 'wallet_outcome_unknown') return 'outcomeUnknown'
  if (code === 'wallet_event_conflict') return 'eventConflict'
  return 'submit'
}
