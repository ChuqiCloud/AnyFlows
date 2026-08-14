import { useState } from 'react'
import { Brain, ChevronDown, Clock3, ScrollText, Zap } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { AdminUsageLog, UserUsageLog } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

export type UsageLogRow = AdminUsageLog | UserUsageLog

type UsageLogTableProps = {
  logs: UsageLogRow[]
  admin: boolean
  filtered: boolean
}

function isAdminLog(log: UsageLogRow): log is AdminUsageLog {
  return 'user_id' in log
}

function useFormatters() {
  const { i18n } = useTranslation()
  return {
    number: (value: number) => new Intl.NumberFormat(i18n.language).format(value),
    date: (value: number) => new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium', timeStyle: 'medium' }).format(value * 1000),
    latency: (value: number | null) => value === null ? '-' : value < 1_000 ? `${value} ms` : `${(value / 1_000).toFixed(value < 10_000 ? 2 : 1)} s`,
  }
}

function ProtocolBadge({ log }: { log: UsageLogRow }) {
  const { t } = useTranslation()
  if (!log.protocol) return <Badge className="bg-surface-2 text-muted-foreground">{t('usageLogs.values.legacy')}</Badge>
  return <Badge className="border-info/20 bg-info/10 text-info">{t(`usageLogs.protocol.${log.protocol}`)}</Badge>
}

function Timing({ log }: { log: UsageLogRow }) {
  const { t } = useTranslation()
  const { latency } = useFormatters()
  return (
    <div className="grid grid-cols-2 gap-x-4 gap-y-1 tabular-nums">
      <span className="text-muted-foreground">{t('usageLogs.values.firstToken')}</span>
      <span>{latency(log.first_token_ms)}</span>
      <span className="text-muted-foreground">{t('usageLogs.values.duration')}</span>
      <span>{latency(log.duration_ms)}</span>
    </div>
  )
}

function TokenUsage({ log }: { log: UsageLogRow }) {
  const { t } = useTranslation()
  const { number } = useFormatters()
  return (
    <div className="grid grid-cols-2 gap-x-4 gap-y-1 tabular-nums">
      <span className="text-muted-foreground">{t('usageLogs.values.inputLabel')}</span>
      <span>{number(log.input_tokens)}</span>
      <span className="text-muted-foreground">{t('usageLogs.values.outputLabel')}</span>
      <span>{number(log.output_tokens)}</span>
    </div>
  )
}

function DetailGrid({ log, admin }: { log: UsageLogRow; admin: boolean }) {
  const { t } = useTranslation()
  const { number } = useFormatters()
  return (
    <dl className="grid gap-x-6 gap-y-3 text-xs sm:grid-cols-2 xl:grid-cols-4">
      <div>
        <dt className="text-muted-foreground">{t('usageLogs.details.request')}</dt>
        <dd className="mt-1 break-all font-mono text-[0.6875rem]">{log.request_id ?? `#${log.id}`}</dd>
      </div>
      <div>
        <dt className="text-muted-foreground">{t('usageLogs.details.tokens')}</dt>
        <dd className="mt-1 tabular-nums">{t('usageLogs.values.cacheReasoning', { cache: number(log.cache_read + log.cache_creation_5m + log.cache_creation_1h), reasoning: number(log.reasoning_tokens) })}</dd>
      </div>
      <div>
        <dt className="text-muted-foreground">{t('usageLogs.details.reasoning')}</dt>
        <dd className="mt-1">{log.reasoning_effort ? t(`usageLogs.reasoning.${log.reasoning_effort}`) : t('usageLogs.values.notSpecified')}{log.reasoning_budget_tokens !== null ? ` / ${number(log.reasoning_budget_tokens)} token` : ''}</dd>
      </div>
      <div>
        <dt className="text-muted-foreground">{t('usageLogs.details.mode')}</dt>
        <dd className="mt-1">{t(`usageLogs.values.billingMode.${log.billing_mode}`)} / {log.is_stream === null ? t('usageLogs.values.legacy') : t(log.is_stream ? 'usageLogs.values.stream' : 'usageLogs.values.sync')}</dd>
      </div>
      {admin && isAdminLog(log) ? (
        <div className="sm:col-span-2 xl:col-span-4">
          <dt className="text-muted-foreground">{t('usageLogs.details.principal')}</dt>
          <dd className="mt-1">{t('usageLogs.values.userTokenGroup', { user: log.user_id, token: log.token_id, group: log.group_id })}</dd>
        </div>
      ) : null}
    </dl>
  )
}

function UsageLogItem({ log, admin }: { log: UsageLogRow; admin: boolean }) {
  const { t } = useTranslation()
  const { date, number } = useFormatters()
  const [expanded, setExpanded] = useState(false)
  return (
    <article className="border-b border-[var(--hairline)] last:border-b-0">
      <div className="grid min-h-16 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-3 py-2.5 md:grid-cols-[minmax(12rem,1.5fr)_9rem_10rem_7rem_2rem]">
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-2">
            <span className="truncate text-sm font-semibold">{log.model ?? t('usageLogs.values.legacyModel')}</span>
            <ProtocolBadge log={log} />
          </div>
          <div className="mt-1 truncate text-[0.6875rem] text-muted-foreground">{date(log.created_at)}{log.operation ? ` / ${t(`usageLogs.operation.${log.operation}`)}` : ''}</div>
        </div>
        <div className="hidden md:block"><Timing log={log} /></div>
        <div className="hidden md:block"><TokenUsage log={log} /></div>
        <div className="hidden text-right tabular-nums md:block">
          <div className="font-medium">{number(log.quota)}</div>
          <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('usageLogs.values.quota')}</div>
        </div>
        <Button type="button" variant="ghost" size="icon-sm" aria-label={t('usageLogs.actions.details')} aria-expanded={expanded} onClick={() => setExpanded((value) => !value)}>
          <ChevronDown className={cn('transition-transform', expanded && 'rotate-180')} aria-hidden="true" />
        </Button>
      </div>
      <div className="grid grid-cols-3 gap-2 px-3 pb-3 md:hidden">
        <div className="rounded-md bg-surface-2/70 px-2 py-1.5"><Clock3 className="mb-1 size-3.5 text-info" aria-hidden="true" /><span className="block text-[0.6875rem] text-muted-foreground">{t('usageLogs.values.duration')}</span><TimingCompact value={log.duration_ms} /></div>
        <div className="rounded-md bg-surface-2/70 px-2 py-1.5"><Zap className="mb-1 size-3.5 text-warning" aria-hidden="true" /><span className="block text-[0.6875rem] text-muted-foreground">Token</span><span className="text-xs font-medium tabular-nums">{number(log.input_tokens + log.output_tokens)}</span></div>
        <div className="rounded-md bg-surface-2/70 px-2 py-1.5"><Brain className="mb-1 size-3.5 text-success" aria-hidden="true" /><span className="block text-[0.6875rem] text-muted-foreground">{t('usageLogs.values.quota')}</span><span className="text-xs font-medium tabular-nums">{number(log.quota)}</span></div>
      </div>
      {expanded ? <div className="border-t border-[var(--hairline)] bg-surface-2/35 px-3 py-3"><DetailGrid log={log} admin={admin} /></div> : null}
    </article>
  )
}

function TimingCompact({ value }: { value: number | null }) {
  const { latency } = useFormatters()
  return <span className="text-xs font-medium tabular-nums">{latency(value)}</span>
}

export function UsageLogTable({ logs, admin, filtered }: UsageLogTableProps) {
  const { t } = useTranslation()
  if (logs.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><ScrollText className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t(filtered ? 'usageLogs.empty.filteredTitle' : 'usageLogs.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(filtered ? 'usageLogs.empty.filteredBody' : 'usageLogs.empty.body')}</p>
        </div>
      </div>
    )
  }
  return (
    <section className="overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1" aria-label={t('usageLogs.listLabel')}>
      <div className="hidden grid-cols-[minmax(12rem,1.5fr)_9rem_10rem_7rem_2rem] gap-3 border-b border-[var(--hairline)] bg-surface-2/60 px-3 py-2 text-[0.6875rem] font-medium text-muted-foreground md:grid">
        <span>{t('usageLogs.columns.call')}</span><span>{t('usageLogs.columns.latency')}</span><span>{t('usageLogs.columns.tokens')}</span><span className="text-right">{t('usageLogs.columns.billing')}</span><span />
      </div>
      {logs.map((log) => <UsageLogItem key={log.id} log={log} admin={admin} />)}
    </section>
  )
}
