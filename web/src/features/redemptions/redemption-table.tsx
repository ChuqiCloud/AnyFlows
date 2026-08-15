import { Ban, Clock3, Ticket } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import type { AdminRedemptionBatch } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type RedemptionTableProps = {
  batches: AdminRedemptionBatch[]
  onDisable: (batch: AdminRedemptionBatch) => void
}

export function RedemptionTable({ batches, onDisable }: RedemptionTableProps) {
  const { i18n, t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'short',
  }), [locale])
  const formatTime = (value: number) => timeFormat.format(value * 1_000)

  if (batches.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
            <Ticket className="size-4" aria-hidden="true" />
          </div>
          <h2 className="mt-3 text-sm font-semibold">{t('redemptions.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('redemptions.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[27%] px-3 py-2 font-medium">{t('redemptions.columns.batch')}</th>
              <th className="w-[18%] px-3 py-2 font-medium">{t('redemptions.columns.value')}</th>
              <th className="w-[24%] px-3 py-2 font-medium">{t('redemptions.columns.progress')}</th>
              <th className="w-[21%] px-3 py-2 font-medium">{t('redemptions.columns.timing')}</th>
              <th className="w-[10%] px-3 py-2"><span className="sr-only">{t('redemptions.columns.actions')}</span></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {batches.map((batch) => (
              <tr key={batch.batch_id} className="hover:bg-surface-2/35">
                <td className="px-3 py-3"><BatchIdentity batch={batch} /></td>
                <td className="px-3 py-3"><BatchValue batch={batch} formatQuota={formatQuota} /></td>
                <td className="px-3 py-3"><RedemptionProgress batch={batch} /></td>
                <td className="px-3 py-3"><BatchTiming batch={batch} formatTime={formatTime} /></td>
                <td className="px-2 py-3"><BatchActions batch={batch} onDisable={onDisable} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {batches.map((batch) => (
          <article key={batch.batch_id} className="min-w-0 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3">
              <BatchIdentity batch={batch} />
              <BatchActions batch={batch} onDisable={onDisable} />
            </div>
            <div className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3">
              <BatchValue batch={batch} formatQuota={formatQuota} />
              <BatchTiming batch={batch} formatTime={formatTime} />
            </div>
            <div className="mt-3 border-t border-[var(--hairline)] pt-3">
              <RedemptionProgress batch={batch} />
            </div>
          </article>
        ))}
      </div>
    </>
  )
}

function BatchIdentity({ batch }: { batch: AdminRedemptionBatch }) {
  const { t } = useTranslation()
  const state = batchState(batch)
  return (
    <div className="min-w-0">
      <div className="flex min-w-0 flex-wrap items-center gap-1.5">
        <span className="truncate font-medium">{batch.name}</span>
        <BatchStateBadge state={state} />
      </div>
      <div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground" title={batch.batch_id}>
        {batch.batch_id.slice(0, 8)} · {t('redemptions.values.creator', { id: batch.created_by_user_id })}
      </div>
    </div>
  )
}

function BatchStateBadge({ state }: { state: ReturnType<typeof batchState> }) {
  const { t } = useTranslation()
  return (
    <Badge className={cn(
      'border-transparent',
      state === 'active' && 'bg-success/10 text-success',
      state === 'expired' && 'bg-warning/10 text-warning',
      state === 'disabled' && 'bg-surface-2 text-muted-foreground',
    )}>
      {t(`redemptions.status.${state}`)}
    </Badge>
  )
}

function BatchValue({
  batch,
  formatQuota,
}: {
  batch: AdminRedemptionBatch
  formatQuota: (value: number) => string
}) {
  const { t } = useTranslation()
  return (
    <div className="tabular-nums">
      <div className="font-medium">{formatQuota(batch.quota_amount)}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {t('redemptions.values.codeCount', { count: batch.code_count })}
      </div>
    </div>
  )
}

function RedemptionProgress({ batch }: { batch: AdminRedemptionBatch }) {
  const { t } = useTranslation()
  const percent = batch.code_count === 0
    ? 0
    : Math.min(100, Math.round(batch.redeemed_count / batch.code_count * 100))
  return (
    <div>
      <div className="flex items-center justify-between gap-3 text-[0.6875rem] tabular-nums">
        <span>{t('redemptions.values.redeemed', { count: batch.redeemed_count, total: batch.code_count })}</span>
        <span className="text-muted-foreground">{percent}%</span>
      </div>
      <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-2" aria-hidden="true">
        <div className="h-full rounded-full bg-brand transition-[width] duration-300" style={{ width: `${percent}%` }} />
      </div>
    </div>
  )
}

function BatchTiming({
  batch,
  formatTime,
}: {
  batch: AdminRedemptionBatch
  formatTime: (value: number) => string
}) {
  const { t } = useTranslation()
  return (
    <div>
      <div className="flex items-center gap-1.5 font-medium">
        <Clock3 className="size-3.5 text-muted-foreground" aria-hidden="true" />
        {batch.expires_at === null ? t('redemptions.values.neverExpires') : formatTime(batch.expires_at)}
      </div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {t('redemptions.values.createdAt', { value: formatTime(batch.created_at) })}
      </div>
    </div>
  )
}

function BatchActions({
  batch,
  onDisable,
}: {
  batch: AdminRedemptionBatch
  onDisable: (batch: AdminRedemptionBatch) => void
}) {
  const { t } = useTranslation()
  if (batch.status === 'disabled') return null
  return (
    <div className="flex justify-end">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            className="size-10 text-muted-foreground hover:text-destructive md:size-8"
            aria-label={t('redemptions.actions.disable')}
            onClick={() => onDisable(batch)}
          >
            <Ban aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t('redemptions.actions.disable')}</TooltipContent>
      </Tooltip>
    </div>
  )
}

function batchState(batch: AdminRedemptionBatch) {
  if (batch.status === 'disabled') return 'disabled' as const
  if (batch.expires_at !== null && batch.expires_at <= Math.floor(Date.now() / 1_000)) {
    return 'expired' as const
  }
  return 'active' as const
}
