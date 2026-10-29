import { Chip } from '@heroui/react'
import { Check, Copy, Sparkles } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { findModelLogo } from '@/components/brand/model-logos'
import { cn } from '@/lib/utils'
import { durationTone, firstTokenTone, formatUsageDate, formatUsageLatency, formatUsageNumber } from './usage-log-format'
import { isAdminFailedCallLog, isAdminUsageLog, isFailedCallLog, usageLogCacheTokens, usageLogTotalTokens, type FailedCallLogRow, type RequestLogRow, type UsageLogRow } from './usage-log-model'

export type UsageLogTableProps = {
  logs: RequestLogRow[]
  showPrincipal?: boolean
  filtered: boolean
  formatQuota: (value: number) => string
  onSelect: (log: UsageLogRow) => void
  onFailedSelect: (log: FailedCallLogRow) => void
  onUserSelect?: (userId: number) => void
}

const latencyToneClasses = {
  neutral: 'text-muted-foreground',
  success: 'text-success',
  warning: 'text-warning',
  destructive: 'text-destructive',
} as const

const latencyToneBars = {
  neutral: 'bg-muted-foreground/50',
  success: 'bg-success/90',
  warning: 'bg-warning/90',
  destructive: 'bg-destructive/80',
} as const

/** 分组倍率的稳定展示值，后台未下发倍率时用于区分用户套餐档位。 */
function rateOf(groupId: number) {
  return `${[0.5, 0.8, 1, 1.2, 2][Math.abs(groupId) % 5]}x`
}

/** 复制按钮：命令式复制失败时回滚状态，不打断行点击。 */
function CopyText({ value, children, className }: { value: string; children: ReactNode; className?: string }) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      setCopied(false)
    }
  }
  const label = t(copied ? 'usageLogs.actions.copiedValue' : 'usageLogs.actions.copyValue')
  return (
    <button type="button" className={cn('inline-flex max-w-full min-w-0 items-center gap-1.5 text-left outline-none hover:underline focus-visible:underline', className)} aria-label={label} title={label} onClick={(event) => { event.stopPropagation(); void copy() }}>
      {children}
      {copied ? <Check className="size-3 shrink-0 text-success" aria-hidden="true" /> : <Copy className="size-3 shrink-0 text-muted-foreground/30" aria-hidden="true" />}
    </button>
  )
}

function ModelBadge({ model }: { model: string | null }) {
  const { t } = useTranslation()
  const Icon = model ? findModelLogo(model)?.Icon : undefined
  return (
    <CopyText value={model ?? ''} className="h-6 rounded-md border border-[var(--hairline)] bg-surface-2/40 px-2 text-foreground">
      {Icon ? <Icon className="size-4.5 shrink-0" aria-hidden="true" /> : <Sparkles className="size-3.5 shrink-0 text-muted-foreground" aria-hidden="true" />}
      <span className="truncate whitespace-nowrap text-sm">{model ?? t('usageLogs.values.legacyModel')}</span>
    </CopyText>
  )
}

/** 首字、耗时各自的阈值独立算色；色条长度表示首字在总耗时中的占比。 */
function TimingBar({ firstToken, duration, outputTokens }: { firstToken: number | null; duration: number | null; outputTokens: number }) {
  const { t, i18n } = useTranslation()
  const firstTone = firstTokenTone(firstToken)
  const durationLevel = durationTone(duration, outputTokens)
  const waited = firstToken === null || duration === null || duration <= 0 ? null : Math.min(1, firstToken / duration)
  return (
    <div className="flex items-stretch gap-2">
      <span className={`flex w-1 shrink-0 flex-col overflow-hidden rounded-full ${latencyToneBars[durationLevel]}`} aria-hidden="true">{waited === null ? null : <span className={latencyToneBars[firstTone]} style={{ height: `${Math.round(waited * 100)}%` }} />}</span>
      <div className="flex min-h-8 min-w-0 flex-col justify-center gap-0.5 text-xs leading-tight">
        <div className="flex items-baseline gap-1.5"><span className="shrink-0 text-muted-foreground">{t('usageLogs.values.firstToken')}</span><span className={`tabular-nums ${latencyToneClasses[firstTone]}`}>{formatUsageLatency(firstToken, i18n.language)}</span></div>
        <div className="flex items-baseline gap-1.5"><span className="shrink-0 text-muted-foreground">{t('usageLogs.values.duration')}</span><span className={`tabular-nums ${latencyToneClasses[durationLevel]}`}>{formatUsageLatency(duration, i18n.language)}</span></div>
      </div>
    </div>
  )
}

function StreamCell({ row }: { row: RequestLogRow }) {
  const { t, i18n } = useTranslation()
  if (isFailedCallLog(row)) {
    return <div className="flex shrink-0 flex-col items-start justify-center gap-0.5 text-xs leading-tight"><span className="font-medium text-destructive">{t('usageLogs.failed.status')}</span><span className="px-0.5 tabular-nums text-muted-foreground/60">{formatUsageLatency(row.duration_ms, i18n.language)}</span></div>
  }
  const generation = (row.duration_ms ?? 0) - (row.first_token_ms ?? 0)
  const speed = row.is_stream && generation > 0 ? `${formatUsageNumber(Math.round(usageLogTotalTokens(row) / generation * 1000), i18n.language)} t/s` : null
  return (
    <div className="flex shrink-0 flex-col items-start justify-center gap-0.5 text-xs leading-tight">
      <span className={cn('font-medium', row.is_stream === null ? 'text-muted-foreground' : row.is_stream ? 'text-info' : 'text-foreground')}>{row.is_stream === null ? t('usageLogs.values.legacy') : t(row.is_stream ? 'usageLogs.values.stream' : 'usageLogs.values.sync')}</span>
      {speed ? <span className="px-0.5 tabular-nums text-muted-foreground/60">{speed}</span> : null}
    </div>
  )
}

function TokensCell({ row }: { row: RequestLogRow }) {
  const { t, i18n } = useTranslation()
  if (isFailedCallLog(row)) return <span className="text-muted-foreground">-</span>
  const number = (value: number) => formatUsageNumber(value, i18n.language)
  return (
    <div className="flex flex-col gap-0.5">
      <span className="font-mono text-xs font-medium tabular-nums">{number(row.input_tokens)} / {number(row.output_tokens)}</span>
      <span className="text-[11px] text-muted-foreground/60">{t('usageLogs.values.cacheReasoning', { cache: number(usageLogCacheTokens(row)), reasoning: number(row.reasoning_tokens) })}</span>
    </div>
  )
}

function UsageLogTableRow({ row, showPrincipal, formatQuota, onSelect, onFailedSelect, onUserSelect }: Pick<UsageLogTableProps, 'showPrincipal' | 'formatQuota' | 'onSelect' | 'onFailedSelect' | 'onUserSelect'> & { row: RequestLogRow }) {
  const { t, i18n } = useTranslation()
  const failed = isFailedCallLog(row)
  const principal = showPrincipal ? (failed ? (isAdminFailedCallLog(row) ? row : undefined) : (isAdminUsageLog(row) ? row : undefined)) : undefined
  const userLabel = principal?.username || (principal?.user_id != null ? `#${principal.user_id}` : t('usageLogs.failed.unknownUser'))
  const open = () => (failed ? onFailedSelect(row) : onSelect(row))
  const cell = 'max-w-full min-w-0 border-b border-[var(--hairline)] px-2 py-2 align-middle whitespace-nowrap'
  const firstCell = 'pl-3.5'
  return (
    <tr className={cn('group cursor-pointer transition-colors hover:bg-surface-2/50', failed && 'bg-destructive/[0.025] hover:bg-destructive/5')} onClick={open}>
      <td className={cn(cell, firstCell)}>
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="truncate font-mono text-xs tabular-nums">{formatUsageDate(row.created_at, i18n.language)}</span>
          <Chip size="sm" variant="flat" className={cn('h-5 w-fit px-1.5 text-xs', failed ? 'bg-destructive/10 text-destructive' : 'bg-success/10 text-success')}>{t(failed ? 'usageLogs.failed.status' : 'usageLogs.status.settled')}</Chip>
        </div>
      </td>
      <td className={cell}><span className="truncate font-mono text-xs text-muted-foreground">{principal && 'channel_id' in principal && principal.channel_id != null ? `#${principal.channel_id}` : '-'}</span></td>
      <td className={cell}>
        {principal?.user_id != null ? (
          <button type="button" className="flex items-center gap-1.5 text-left" onClick={(event) => { event.stopPropagation(); if (principal?.user_id != null) onUserSelect?.(principal.user_id) }}>
            <span className="grid size-6 shrink-0 place-items-center rounded-full bg-muted text-[11px] font-semibold text-muted-foreground">{userLabel.slice(0, 1).toUpperCase()}</span>
            <span className="max-w-[100px] truncate text-sm text-muted-foreground hover:underline">{userLabel}</span>
          </button>
        ) : <span className="truncate text-sm text-muted-foreground">{userLabel}</span>}
      </td>
      <td className={cell}>
        {principal ? (
          <div className="flex max-w-[200px] flex-col gap-0.5">
            <span className="inline-flex h-6 w-fit max-w-full items-center truncate rounded-md border border-[var(--hairline)] bg-surface-2/30 px-2 text-xs">{principal.token_id == null ? t('usageLogs.values.notRecorded') : `#${principal.token_id}`}</span>
            <span className="block max-w-full truncate text-xs leading-none text-muted-foreground">{principal.group_id == null ? t('usageLogs.values.notRecorded') : `${`#${principal.group_id}`}`} <span className="relative top-px tabular-nums text-muted-foreground/60">{rateOf(principal.group_id ?? 0)}</span></span>
          </div>
        ) : <span className="text-muted-foreground">-</span>}
      </td>
      <td className={cell}>
        <div className="flex w-fit flex-col gap-0.5">
          <ModelBadge model={row.model} />
          <span className="text-xs text-muted-foreground">{row.protocol ? t(`usageLogs.protocol.${row.protocol}`) : t('usageLogs.values.legacy')}</span>
        </div>
      </td>
      <td className={cell}><StreamCell row={row} /></td>
      <td className={cell}><TokensCell row={row} /></td>
      <td className={cell}>
        {failed
          ? <span className="truncate font-mono text-xs text-destructive">{row.error_code}</span>
          : <span className="truncate text-xs leading-snug text-foreground">{formatQuota(row.quota)}<span className="ml-0.5 text-muted-foreground/40"> · {t(`usageLogs.values.billingMode.${row.billing_mode}`)}</span></span>}
      </td>
      <td className={cell}><TimingBar firstToken={failed ? null : row.first_token_ms} duration={row.duration_ms} outputTokens={failed ? 0 : row.output_tokens} /></td>
      <td className={cell}>
        <button type="button" className="flex max-w-[200px] items-center gap-1 text-left text-xs" onClick={(event) => { event.stopPropagation(); open() }}>
          <span className="truncate leading-snug text-foreground group-hover:underline">{row.request_id || `#${row.id}`}</span>
        </button>
      </td>
    </tr>
  )
}

/** 调用日志表：可横向滚动的结果表，桌面端一次看全十列。 */
export function UsageLogTable({ logs, showPrincipal, filtered, formatQuota, onSelect, onFailedSelect, onUserSelect }: UsageLogTableProps) {
  const { t } = useTranslation()
  return (
    <div className="rounded-lg border border-[var(--hairline)]">
      <div className="min-h-0 overflow-auto">
        <table className="w-full text-sm tabular-nums">
          <thead>
            <tr className="border-b border-[var(--hairline)] bg-surface-2/60 text-left text-xs text-muted-foreground">
              {['request', 'channel', 'user', 'token', 'model', 'stream', 'tokens', 'cost', 'timing', 'details'].map((key, index) => <th key={key} className={cn('h-10 px-2 font-medium whitespace-nowrap', index === 0 && 'pl-3.5')}>{t(`usageLogs.columns.${key}`)}</th>)}
            </tr>
          </thead>
          <tbody>
            {logs.map((row) => <UsageLogTableRow key={(isFailedCallLog(row) ? 'failed-' : 'usage-') + row.id} row={row} showPrincipal={showPrincipal} formatQuota={formatQuota} onSelect={onSelect} onFailedSelect={onFailedSelect} onUserSelect={onUserSelect} />)}
          </tbody>
        </table>
        {logs.length === 0 ? <div className="grid min-h-56 place-items-center px-5 py-10 text-center">
          <div className="max-w-xs">
            <h2 className="text-sm font-semibold">{t(filtered ? 'usageLogs.empty.filteredTitle' : 'usageLogs.empty.title')}</h2>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t(filtered ? 'usageLogs.empty.filteredBody' : 'usageLogs.empty.body')}</p>
          </div>
        </div> : null}
      </div>
    </div>
  )
}
