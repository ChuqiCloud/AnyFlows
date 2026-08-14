import { ChevronRight, CircleCheck, Clock3, Coins, ScrollText, Zap } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { formatUsageDate, formatUsageLatency, formatUsageNumber } from './usage-log-format'
import { isAdminUsageLog, type UsageLogRow, usageLogTotalTokens } from './usage-log-model'

type UsageLogTableProps = {
  logs: UsageLogRow[]
  admin: boolean
  filtered: boolean
  onSelect: (log: UsageLogRow) => void
}

function ProtocolBadge({ log }: { log: UsageLogRow }) {
  const { t } = useTranslation()
  if (!log.protocol) return <Badge className="shrink-0 text-muted-foreground">{t('usageLogs.values.legacy')}</Badge>
  return <Badge className="shrink-0 border-transparent bg-info/10 text-info">{t(`usageLogs.protocol.${log.protocol}`)}</Badge>
}

function UsageLogItem({ log, admin, onSelect }: { log: UsageLogRow; admin: boolean; onSelect: (log: UsageLogRow) => void }) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const number = (value: number) => formatUsageNumber(value, locale)

  return (
    <button
      type="button"
      className="group grid w-full gap-3 border-b border-[var(--hairline)] px-3 py-3 text-left outline-none transition-colors last:border-b-0 hover:bg-surface-2/45 focus-visible:bg-surface-2 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50 lg:min-h-[4.5rem] lg:grid-cols-[7rem_minmax(10rem,1.4fr)_8.25rem_10rem_7.5rem_1.25rem] lg:items-center lg:px-4 lg:py-2.5"
      aria-label={t('usageLogs.actions.openDetails', { model: log.model ?? t('usageLogs.values.legacyModel') })}
      onClick={() => onSelect(log)}
    >
      <div className="hidden text-xs text-muted-foreground lg:block">
        <div className="tabular-nums text-foreground">{formatUsageDate(log.created_at, locale)}</div>
        <div className="mt-1 inline-flex items-center gap-1 text-[0.6875rem] text-success">
          <CircleCheck className="size-3" aria-hidden="true" />
          {t('usageLogs.status.settled')}
        </div>
      </div>

      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-semibold">{log.model ?? t('usageLogs.values.legacyModel')}</span>
          <ProtocolBadge log={log} />
        </div>
        <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[0.6875rem] text-muted-foreground">
          <span>{log.operation ? t(`usageLogs.operation.${log.operation}`) : t('usageLogs.values.notSpecified')}</span>
          <span aria-hidden="true">·</span>
          <span>{log.is_stream === null ? t('usageLogs.values.legacy') : t(log.is_stream ? 'usageLogs.values.stream' : 'usageLogs.values.sync')}</span>
          {admin && isAdminUsageLog(log) ? (
            <>
              <span aria-hidden="true">·</span>
              <span className="truncate">{t('usageLogs.values.userTokenGroupCompact', { user: log.user_id, token: log.token_id, group: log.group_id })}</span>
            </>
          ) : null}
        </div>
      </div>

      <div className="hidden grid-cols-[auto_1fr] gap-x-2 gap-y-1 text-xs tabular-nums lg:grid">
        <span className="text-muted-foreground">{t('usageLogs.values.firstToken')}</span>
        <span className="text-right">{formatUsageLatency(log.first_token_ms, locale)}</span>
        <span className="text-muted-foreground">{t('usageLogs.values.duration')}</span>
        <span className="text-right">{formatUsageLatency(log.duration_ms, locale)}</span>
      </div>

      <div className="hidden min-w-0 lg:block">
        <div className="text-sm font-semibold tabular-nums">{number(usageLogTotalTokens(log))}</div>
        <div className="mt-1 truncate text-[0.6875rem] tabular-nums text-muted-foreground">
          {t('usageLogs.values.inputOutputCompact', { input: number(log.input_tokens), output: number(log.output_tokens) })}
        </div>
      </div>

      <div className="hidden text-right lg:block">
        <div className="text-sm font-semibold tabular-nums">{number(log.quota)}</div>
        <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t(`usageLogs.values.billingMode.${log.billing_mode}`)}</div>
      </div>

      <ChevronRight className="hidden size-4 text-muted-foreground transition-transform group-hover:translate-x-0.5 lg:block" aria-hidden="true" />

      <div className="flex min-w-0 items-center justify-between gap-3 lg:hidden">
        <div className="flex min-w-0 items-center gap-2 text-[0.6875rem] text-muted-foreground">
          <span className="inline-flex shrink-0 items-center gap-1 text-success"><CircleCheck className="size-3" aria-hidden="true" />{t('usageLogs.status.settled')}</span>
          <span className="truncate tabular-nums">{formatUsageDate(log.created_at, locale)}</span>
        </div>
        <ChevronRight className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
      </div>

      <div className="grid grid-cols-3 divide-x divide-[var(--hairline)] border-t border-[var(--hairline)] pt-3 lg:hidden">
        <div className="min-w-0 pr-2">
          <span className="flex items-center gap-1 text-[0.6875rem] text-muted-foreground"><Clock3 className="size-3" aria-hidden="true" />{t('usageLogs.values.duration')}</span>
          <span className="mt-1 block truncate text-xs font-semibold tabular-nums">{formatUsageLatency(log.duration_ms, locale)}</span>
        </div>
        <div className="min-w-0 px-2">
          <span className="flex items-center gap-1 text-[0.6875rem] text-muted-foreground"><Zap className="size-3" aria-hidden="true" />Token</span>
          <span className="mt-1 block truncate text-xs font-semibold tabular-nums">{number(usageLogTotalTokens(log))}</span>
        </div>
        <div className="min-w-0 pl-2">
          <span className="flex items-center gap-1 text-[0.6875rem] text-muted-foreground"><Coins className="size-3" aria-hidden="true" />{t('usageLogs.values.quotaShort')}</span>
          <span className="mt-1 block truncate text-xs font-semibold tabular-nums">{number(log.quota)}</span>
        </div>
      </div>
    </button>
  )
}

export function UsageLogTable({ logs, admin, filtered, onSelect }: UsageLogTableProps) {
  const { t } = useTranslation()
  if (logs.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center rounded-lg border border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs px-5">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><ScrollText className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t(filtered ? 'usageLogs.empty.filteredTitle' : 'usageLogs.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(filtered ? 'usageLogs.empty.filteredBody' : 'usageLogs.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <section className="overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1" aria-label={t('usageLogs.listLabel')}>
      <div className="hidden grid-cols-[7rem_minmax(10rem,1.4fr)_8.25rem_10rem_7.5rem_1.25rem] gap-3 border-b border-[var(--hairline)] bg-surface-2/60 px-4 py-2 text-[0.6875rem] font-medium text-muted-foreground lg:grid">
        <span>{t('usageLogs.columns.time')}</span>
        <span>{t('usageLogs.columns.call')}</span>
        <span>{t('usageLogs.columns.latency')}</span>
        <span>{t('usageLogs.columns.tokens')}</span>
        <span className="text-right">{t('usageLogs.columns.billing')}</span>
        <span />
      </div>
      {logs.map((log) => <UsageLogItem key={log.id} log={log} admin={admin} onSelect={onSelect} />)}
    </section>
  )
}
