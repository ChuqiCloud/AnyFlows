import { useState, type MouseEvent } from 'react'
import { Check, ChevronRight, CircleAlert, CircleCheck, Clock3, Coins, Copy, ScrollText, UserRound, Zap } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import {
  formatCompactRequestId,
  formatUsageDate,
  formatUsageLatency,
  formatUsageNumber,
  usageLatencyTone,
  type UsageLatencyKind,
} from './usage-log-format'
import { isAdminFailedCallLog, isAdminUsageLog, isFailedCallLog, type RequestLogRow, type UsageLogRow, usageLogTotalTokens } from './usage-log-model'

type UsageLogTableProps = {
  logs: RequestLogRow[]
  admin: boolean
  showPrincipal?: boolean
  filtered: boolean
  formatQuota: (value: number) => string
  onSelect: (log: UsageLogRow) => void
  onUserSelect?: (userId: number) => void
}

function ProtocolBadge({ log }: { log: UsageLogRow }) {
  const { t } = useTranslation()
  if (!log.protocol) return <Badge className="shrink-0 text-muted-foreground">{t('usageLogs.values.legacy')}</Badge>
  return <Badge className="shrink-0 border-transparent bg-info/10 text-info">{t(`usageLogs.protocol.${log.protocol}`)}</Badge>
}

const latencyToneClasses = {
  neutral: 'text-muted-foreground',
  success: 'text-success',
  warning: 'text-warning',
  destructive: 'text-destructive',
} as const

function latencyClass(value: number | null, kind: UsageLatencyKind) {
  return latencyToneClasses[usageLatencyTone(value, kind)]
}

function RequestIdCopy({ requestId, onClick }: { requestId: string; onClick: (event: MouseEvent) => void }) {
  const { t } = useTranslation()
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle')

  const copy = async (event: MouseEvent) => {
    onClick(event)
    try {
      await navigator.clipboard.writeText(requestId)
      setState('copied')
    } catch {
      setState('failed')
    }
  }

  const label = state === 'copied'
    ? t('usageLogs.actions.copiedRequestId')
    : state === 'failed'
      ? t('usageLogs.actions.copyRequestIdFailed')
      : t('usageLogs.actions.copyRequestId')

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button type="button" size="sm" variant="ghost" className={cn('h-6 min-w-0 max-w-48 gap-1 px-1.5 font-mono text-[0.6875rem] font-normal text-muted-foreground', state === 'failed' && 'text-destructive')} aria-label={label} onClick={(event) => void copy(event)}>
          <span className="truncate">{formatCompactRequestId(requestId)}</span>
          {state === 'copied' ? <Check className="size-3 shrink-0 text-success" aria-hidden="true" /> : <Copy className="size-3 shrink-0" aria-hidden="true" />}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}

function UsageLogItem({ log, showPrincipal, formatQuota, onSelect, onUserSelect }: {
  log: RequestLogRow
  showPrincipal: boolean
  formatQuota: (value: number) => string
  onSelect: (log: UsageLogRow) => void
  onUserSelect?: (userId: number) => void
}) {
  const { t, i18n } = useTranslation()
  const locale = i18n.language
  const number = (value: number) => formatUsageNumber(value, locale)
  const stop = (event: MouseEvent) => event.stopPropagation()

  if (isFailedCallLog(log)) {
    const internal = isAdminFailedCallLog(log)
    return (
      <article className="grid w-full gap-3 border-b border-[var(--hairline)] bg-destructive/[0.025] px-3 py-3 text-left last:border-b-0 lg:min-h-[4.5rem] lg:grid-cols-[7rem_minmax(11rem,1.4fr)_minmax(14rem,1fr)_8.25rem_2rem] lg:items-center lg:px-4 lg:py-2.5">
        <div className="text-xs text-muted-foreground"><div className="tabular-nums text-foreground">{formatUsageDate(log.created_at, locale)}</div><div className="mt-1 inline-flex items-center gap-1 text-[0.6875rem] text-destructive"><CircleAlert className="size-3" aria-hidden="true" />{t('usageLogs.failed.status')}</div></div>
        <div className="min-w-0"><div className="flex min-w-0 flex-wrap items-center gap-2"><span className="truncate text-sm font-semibold">{log.model}</span><Badge className="shrink-0 border-transparent bg-info/10 text-info">{t('usageLogs.protocol.' + log.protocol)}</Badge></div><div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[0.6875rem] text-muted-foreground"><span>{t('usageLogs.operation.' + log.operation)}</span>{log.request_id ? <RequestIdCopy requestId={log.request_id} onClick={stop} /> : null}</div></div>
        <div className="min-w-0 rounded-md bg-destructive/10 px-2.5 py-2"><div className="flex flex-wrap items-center gap-2"><Badge className="border-transparent bg-destructive/10 font-mono text-destructive">{log.error_code}</Badge>{internal && showPrincipal ? <span className="truncate font-mono text-[0.6875rem] text-muted-foreground">{log.error_kind}</span> : null}</div><p className="mt-1 break-words text-xs leading-5 text-foreground">{log.error_message}</p></div>
        <div className="text-xs font-medium tabular-nums text-muted-foreground"><Clock3 className="mr-1 inline size-3" aria-hidden="true" />{formatUsageLatency(log.duration_ms, locale)}</div>
        <div className="hidden lg:block"><Badge className="border-transparent bg-destructive/10 text-destructive">{t('usageLogs.failed.badge')}</Badge></div>
      </article>
    )
  }

  return (
    <article
      className="group grid w-full cursor-pointer gap-3 border-b border-[var(--hairline)] px-3 py-3 text-left transition-colors last:border-b-0 hover:bg-surface-2/45 lg:min-h-[4.5rem] lg:grid-cols-[7rem_minmax(11rem,1.4fr)_8.25rem_10rem_7.5rem_2rem] lg:items-center lg:px-4 lg:py-2.5"
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
          <button type="button" className="truncate text-sm font-semibold outline-none hover:text-brand focus-visible:text-brand focus-visible:underline" onClick={(event) => { stop(event); onSelect(log) }}>
            {log.model ?? t('usageLogs.values.legacyModel')}
          </button>
          <ProtocolBadge log={log} />
        </div>
        <div className="mt-1 flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[0.6875rem] text-muted-foreground">
          <span>{log.operation ? t(`usageLogs.operation.${log.operation}`) : t('usageLogs.values.notSpecified')}</span>
          <span>{log.is_stream === null ? t('usageLogs.values.legacy') : t(log.is_stream ? 'usageLogs.values.stream' : 'usageLogs.values.sync')}</span>
          {showPrincipal && isAdminUsageLog(log) ? onUserSelect ? (
            <button type="button" className="inline-flex min-w-0 items-center gap-1 font-medium text-foreground outline-none hover:text-brand focus-visible:text-brand focus-visible:underline" onClick={(event) => { stop(event); onUserSelect(log.user_id) }}>
              <UserRound className="size-3 shrink-0" aria-hidden="true" />
              <span className="truncate">{log.username}</span>
            </button>
          ) : (
            <span className="inline-flex min-w-0 items-center gap-1 font-medium text-foreground">
              <UserRound className="size-3 shrink-0" aria-hidden="true" />
              <span className="truncate">{log.username}</span>
            </span>
          ) : null}
          {log.request_id ? <RequestIdCopy requestId={log.request_id} onClick={stop} /> : null}
        </div>
      </div>

      <div className="hidden grid-cols-[auto_1fr] gap-x-2 gap-y-1 text-xs tabular-nums lg:grid">
        <span className="text-muted-foreground">{t('usageLogs.values.firstToken')}</span>
        <span className={cn('text-right font-medium', latencyClass(log.first_token_ms, 'first-token'))}>{formatUsageLatency(log.first_token_ms, locale)}</span>
        <span className="text-muted-foreground">{t('usageLogs.values.duration')}</span>
        <span className={cn('text-right font-medium', latencyClass(log.duration_ms, 'duration'))}>{formatUsageLatency(log.duration_ms, locale)}</span>
      </div>

      <div className="hidden min-w-0 lg:block">
        <div className="text-sm font-semibold tabular-nums">{number(usageLogTotalTokens(log))}</div>
        <div className="mt-1 truncate text-[0.6875rem] tabular-nums text-muted-foreground">
          {t('usageLogs.values.inputOutputCompact', { input: number(log.input_tokens), output: number(log.output_tokens) })}
        </div>
      </div>

      <div className="hidden text-right lg:block">
        <div className="text-sm font-semibold tabular-nums">{formatQuota(log.quota)}</div>
        <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t(`usageLogs.values.billingMode.${log.billing_mode}`)}</div>
      </div>

      <Button type="button" size="icon-sm" variant="ghost" className="hidden lg:inline-flex" aria-label={t('usageLogs.actions.openDetails', { model: log.model ?? t('usageLogs.values.legacyModel') })} onClick={(event) => { stop(event); onSelect(log) }}>
        <ChevronRight className="size-4 transition-transform group-hover:translate-x-0.5" aria-hidden="true" />
      </Button>

      <div className="flex min-w-0 items-center justify-between gap-3 lg:hidden">
        <div className="flex min-w-0 items-center gap-2 text-[0.6875rem] text-muted-foreground">
          <span className="inline-flex shrink-0 items-center gap-1 text-success"><CircleCheck className="size-3" aria-hidden="true" />{t('usageLogs.status.settled')}</span>
          <span className="truncate tabular-nums">{formatUsageDate(log.created_at, locale)}</span>
        </div>
        <Button type="button" size="icon-sm" variant="ghost" aria-label={t('usageLogs.actions.openDetails', { model: log.model ?? t('usageLogs.values.legacyModel') })} onClick={(event) => { stop(event); onSelect(log) }}><ChevronRight className="size-4" aria-hidden="true" /></Button>
      </div>

      <div className="grid grid-cols-3 divide-x divide-[var(--hairline)] border-t border-[var(--hairline)] pt-3 lg:hidden">
        <div className="min-w-0 pr-2">
          <span className="flex items-center gap-1 text-[0.6875rem] text-muted-foreground"><Clock3 className="size-3" aria-hidden="true" />{t('usageLogs.columns.latency')}</span>
          <span className="mt-1 block truncate text-[0.6875rem] font-semibold tabular-nums" title={`${formatUsageLatency(log.first_token_ms, locale)} / ${formatUsageLatency(log.duration_ms, locale)}`}>
            <span className={latencyClass(log.first_token_ms, 'first-token')}>{formatUsageLatency(log.first_token_ms, locale)}</span>
            <span className="text-muted-foreground"> / </span>
            <span className={latencyClass(log.duration_ms, 'duration')}>{formatUsageLatency(log.duration_ms, locale)}</span>
          </span>
        </div>
        <div className="min-w-0 px-2">
          <span className="flex items-center gap-1 text-[0.6875rem] text-muted-foreground"><Zap className="size-3" aria-hidden="true" />Token</span>
          <span className="mt-1 block truncate text-xs font-semibold tabular-nums">{number(usageLogTotalTokens(log))}</span>
        </div>
        <div className="min-w-0 pl-2">
          <span className="flex items-center gap-1 text-[0.6875rem] text-muted-foreground"><Coins className="size-3" aria-hidden="true" />{t('usageLogs.values.quotaShort')}</span>
          <span className="mt-1 block truncate text-xs font-semibold tabular-nums">{formatQuota(log.quota)}</span>
        </div>
      </div>
    </article>
  )
}

export function UsageLogTable({ logs, admin, showPrincipal = admin, filtered, formatQuota, onSelect, onUserSelect }: UsageLogTableProps) {
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
      <div className="hidden grid-cols-[7rem_minmax(11rem,1.4fr)_8.25rem_10rem_7.5rem_2rem] gap-3 border-b border-[var(--hairline)] bg-surface-2/60 px-4 py-2 text-[0.6875rem] font-medium text-muted-foreground lg:grid">
        <span>{t('usageLogs.columns.time')}</span>
        <span>{t('usageLogs.columns.call')}</span>
        <span>{t('usageLogs.columns.latency')}</span>
        <span>{t('usageLogs.columns.tokens')}</span>
        <span className="text-right">{t('usageLogs.columns.billing')}</span>
        <span />
      </div>
      {logs.map((log) => <UsageLogItem key={(isFailedCallLog(log) ? 'failed-' : 'usage-') + log.id} log={log} showPrincipal={showPrincipal} formatQuota={formatQuota} onSelect={onSelect} onUserSelect={onUserSelect} />)}
    </section>
  )
}
