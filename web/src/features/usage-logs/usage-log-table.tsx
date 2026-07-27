import { ScrollText } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import type { AdminUsageLog } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type UsageLogTableProps = {
  logs: AdminUsageLog[]
}

function useFormatters() {
  const { i18n } = useTranslation()
  return {
    number: (value: number) => new Intl.NumberFormat(i18n.language).format(value),
    date: (value: number) => new Intl.DateTimeFormat(i18n.language, {
      dateStyle: 'medium',
      timeStyle: 'medium',
    }).format(value * 1000),
  }
}

function SourceBadge({ log }: { log: AdminUsageLog }) {
  const { t } = useTranslation()
  return (
    <Badge className={cn(
      'border-transparent',
      log.usage_source === 'upstream'
        ? 'bg-success/10 text-success'
        : 'bg-warning/10 text-warning',
    )}>
      {t('usageLogs.values.source.' + log.usage_source)}
    </Badge>
  )
}

function CoreUsage({ log }: { log: AdminUsageLog }) {
  const { t } = useTranslation()
  const { number } = useFormatters()
  return (
    <>
      <div>{t('usageLogs.values.input', { value: number(log.input_tokens) })}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {t('usageLogs.values.output', { value: number(log.output_tokens) })}
      </div>
    </>
  )
}

function DetailedUsage({ log }: { log: AdminUsageLog }) {
  const { t } = useTranslation()
  const { number } = useFormatters()
  return (
    <>
      <div>{t('usageLogs.values.cacheRead', { value: number(log.cache_read) })}</div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {t('usageLogs.values.cacheCreate', {
          five: number(log.cache_creation_5m),
          hour: number(log.cache_creation_1h),
        })}
      </div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {t('usageLogs.values.reasoningAudio', {
          reasoning: number(log.reasoning_tokens),
          audio: number(log.audio_input_tokens + log.audio_output_tokens),
        })}
      </div>
    </>
  )
}

function BillingValue({ log }: { log: AdminUsageLog }) {
  const { t } = useTranslation()
  const { number } = useFormatters()
  return (
    <>
      <div className="flex items-center gap-2">
        <span>{t('usageLogs.values.billingMode.' + log.billing_mode)}</span>
        <SourceBadge log={log} />
      </div>
      <div className="mt-1 text-[0.6875rem] text-muted-foreground">
        {t('usageLogs.values.quotaSemantics', {
          quota: number(log.quota),
          semantics: t('usageLogs.values.semantics.' + log.usage_semantics),
        })}
      </div>
    </>
  )
}

export function UsageLogTable({ logs }: UsageLogTableProps) {
  const { t } = useTranslation()
  const { date } = useFormatters()
  if (logs.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
            <ScrollText className="size-4" aria-hidden="true" />
          </div>
          <h2 className="mt-3 text-sm font-semibold">{t('usageLogs.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('usageLogs.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr>
              <th className="w-[24%] px-3 py-2 font-medium">{t('usageLogs.columns.event')}</th>
              <th className="w-[16%] px-3 py-2 font-medium">{t('usageLogs.columns.principal')}</th>
              <th className="w-[18%] px-3 py-2 font-medium">{t('usageLogs.columns.usage')}</th>
              <th className="w-[20%] px-3 py-2 font-medium">{t('usageLogs.columns.details')}</th>
              <th className="w-[22%] px-3 py-2 font-medium">{t('usageLogs.columns.billing')}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {logs.map((log) => (
              <tr key={log.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5">
                  <div className="font-medium tabular-nums">{date(log.created_at)}</div>
                  <div title={log.event_id} className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">
                    {log.event_id} · #{log.id}
                  </div>
                </td>
                <td className="px-3 py-2.5">
                  <div>{t('usageLogs.values.userToken', { user: log.user_id, token: log.token_id })}</div>
                  <div className="mt-1 text-[0.6875rem] text-muted-foreground">
                    {t('usageLogs.values.group', { id: log.group_id })}
                  </div>
                </td>
                <td className="px-3 py-2.5 tabular-nums"><CoreUsage log={log} /></td>
                <td className="px-3 py-2.5 tabular-nums"><DetailedUsage log={log} /></td>
                <td className="px-3 py-2.5 tabular-nums"><BillingValue log={log} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {logs.map((log) => (
          <article key={log.id} className="rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="min-w-0">
              <div className="text-sm font-semibold tabular-nums">{date(log.created_at)}</div>
              <p title={log.event_id} className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground">
                {log.event_id} · #{log.id}
              </p>
            </div>
            <dl className="mt-3 grid grid-cols-2 gap-3 border-t border-[var(--hairline)] pt-3 text-xs">
              <div>
                <dt className="text-muted-foreground">{t('usageLogs.columns.principal')}</dt>
                <dd className="mt-1">{t('usageLogs.values.userToken', { user: log.user_id, token: log.token_id })}</dd>
              </div>
              <div>
                <dt className="text-muted-foreground">{t('usageLogs.columns.usage')}</dt>
                <dd className="mt-1 tabular-nums"><CoreUsage log={log} /></dd>
              </div>
              <div>
                <dt className="text-muted-foreground">{t('usageLogs.columns.details')}</dt>
                <dd className="mt-1 tabular-nums"><DetailedUsage log={log} /></dd>
              </div>
              <div>
                <dt className="text-muted-foreground">{t('usageLogs.columns.billing')}</dt>
                <dd className="mt-1 tabular-nums"><BillingValue log={log} /></dd>
              </div>
            </dl>
          </article>
        ))}
      </div>
    </>
  )
}
