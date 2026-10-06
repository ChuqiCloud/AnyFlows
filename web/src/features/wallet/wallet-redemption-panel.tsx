import { zodResolver } from '@hookform/resolvers/zod'
import { CheckCircle2, LoaderCircle, TicketCheck } from 'lucide-react'
import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'
import { z } from 'zod'

import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { ApiError } from '@/lib/api'
import type { UserRedemptionResult } from '@/lib/api/generated/types.gen'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useRedeemUserCode } from './wallet-api'

type RedemptionValues = {
  code: string
}

export function WalletRedemptionPanel() {
  const { t } = useTranslation()
  const mutation = useRedeemUserCode()
  const { formatQuota } = useBalanceDisplay()
  const [result, setResult] = useState<UserRedemptionResult>()
  const form = useForm<RedemptionValues>({
    defaultValues: { code: '' },
    resolver: zodResolver(z.object({
      code: z.string().regex(/^rc-af-[A-Za-z0-9_-]{43}$/, t('wallet.redemption.validation')),
    })),
  })

  const submit = form.handleSubmit(async ({ code }) => {
    const redeemed = await mutation.mutateAsync({ code })
    setResult(redeemed)
    form.reset()
  })
  const errorKey = redemptionErrorKey(apiErrorCode(mutation.error))

  return (
    <Card>
      <CardContent className="p-4">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-end lg:justify-between">
          <div className="flex min-w-0 gap-3">
            <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand">
              <TicketCheck className="size-4" aria-hidden="true" />
            </span>
            <div className="min-w-0">
              <h3 className="text-sm font-semibold">{t('wallet.redemption.title')}</h3>
              <p className="mt-1 max-w-xl text-xs leading-5 text-muted-foreground">{t('wallet.redemption.description')}</p>
            </div>
          </div>
          <form className="flex w-full flex-col gap-2 sm:flex-row lg:max-w-xl" onSubmit={(event) => void submit(event)}>
            <div className="min-w-0 flex-1">
              <Label htmlFor="wallet-redemption-code" className="sr-only">{t('wallet.redemption.field')}</Label>
              <Input
                id="wallet-redemption-code"
                className="font-mono text-xs"
                placeholder="rc-af-…"
                autoComplete="off"
                autoCapitalize="none"
                spellCheck={false}
                disabled={mutation.isPending}
                {...form.register('code', {
                  onChange: () => {
                    setResult(undefined)
                    mutation.reset()
                  },
                })}
              />
              {form.formState.errors.code ? (
                <p role="alert" className="mt-1 text-xs text-destructive">{form.formState.errors.code.message}</p>
              ) : null}
            </div>
            <Button type="submit" className="shrink-0" disabled={mutation.isPending}>
              {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <TicketCheck aria-hidden="true" />}
              {t(mutation.isPending ? 'wallet.redemption.redeeming' : 'wallet.redemption.submit')}
            </Button>
          </form>
        </div>

        {result ? (
          <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-[var(--hairline)] pt-3 text-xs text-success" role="status">
            <CheckCircle2 className="size-4" aria-hidden="true" />
            <span>{t(result.replayed ? 'wallet.redemption.replayed' : 'wallet.redemption.success', {
              amount: formatQuota(result.quota_amount),
            })}</span>
            <span className="text-muted-foreground">{t('wallet.redemption.balanceAfter', {
              balance: formatQuota(result.balance_after),
            })}</span>
          </div>
        ) : mutation.isError ? (
          <p role="alert" className="mt-3 border-t border-[var(--hairline)] pt-3 text-xs text-destructive">
            {t(`wallet.redemption.errors.${errorKey}`)}
          </p>
        ) : null}
      </CardContent>
    </Card>
  )
}

function apiErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}

function redemptionErrorKey(code: string | undefined) {
  if (code === 'redemption_code_invalid') return 'invalid'
  if (code === 'redemption_batch_disabled') return 'disabled'
  if (code === 'redemption_code_expired') return 'expired'
  if (code === 'redemption_code_already_used') return 'used'
  if (code === 'wallet_overflow') return 'overflow'
  if (code === 'redemption_outcome_unknown') return 'outcomeUnknown'
  return 'unknown'
}
