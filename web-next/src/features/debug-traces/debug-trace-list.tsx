import { Activity, ChevronRight, Clock3, Route } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Chip } from '@heroui/react'
import type { AdminDebugTrace } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type DebugTraceListProps = {
  traces: readonly AdminDebugTrace[]
  selectedId?: number
  onSelect: (traceId: number) => void
}

/** 以桌面日志表和移动端紧凑条目呈现追踪摘要，详情统一交给侧栏承载。 */
export function DebugTraceList({ traces, selectedId, onSelect }: DebugTraceListProps) {
  const { t, i18n } = useTranslation()
  if (traces.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center rounded-lg border border-[var(--hairline)] px-6 py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
            <Activity className="size-4" aria-hidden="true" />
          </div>
          <h3 className="mt-3 text-sm font-semibold">{t('debugTraces.empty.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('debugTraces.empty.body')}</p>
        </div>
      </div>
    )
  }

  const dateFormatter = new Intl.DateTimeFormat(i18n.language, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })

  return (
    <section className="@container overflow-hidden rounded-lg border border-[var(--hairline)] bg-surface-1" aria-label={t('debugTraces.listLabel')}>
      <div className="hidden grid-cols-[7rem_minmax(10rem,1.35fr)_7.5rem_7.25rem_8.5rem_5.5rem_2rem] gap-3 border-b border-[var(--hairline)] bg-surface-2/60 px-4 py-2 text-[0.6875rem] font-medium text-muted-foreground @min-[54rem]:grid">
        <span>{t('debugTraces.columns.time')}</span>
        <span>{t('debugTraces.columns.request')}</span>
        <span>{t('debugTraces.columns.protocol')}</span>
        <span>{t('debugTraces.columns.routing')}</span>
        <span>{t('debugTraces.columns.principal')}</span>
        <span>{t('debugTraces.columns.outcome')}</span>
        <span />
      </div>
      {traces.map((trace) => {
        const selected = selectedId === trace.id
        const converted = trace.downstream_protocol !== trace.upstream_protocol
        return (
          <button
            key={trace.id}
            type="button"
            aria-pressed={selected}
            aria-label={t('debugTraces.actions.openDetails', { model: trace.requested_model })}
            className={cn(
              'group grid w-full gap-2 border-b border-[var(--hairline)] px-3 py-3 text-left outline-none transition-colors last:border-b-0',
              'hover:bg-surface-2/45 focus-visible:bg-surface-2 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50',
              '@min-[54rem]:min-h-[4.25rem] @min-[54rem]:grid-cols-[7rem_minmax(10rem,1.35fr)_7.5rem_7.25rem_8.5rem_5.5rem_2rem] @min-[54rem]:items-center @min-[54rem]:gap-3 @min-[54rem]:px-4 @min-[54rem]:py-2.5',
              selected && 'bg-brand/7 shadow-[inset_2px_0_0_var(--brand)]',
            )}
            onClick={() => onSelect(trace.id)}
          >
            <div className="hidden text-xs tabular-nums text-muted-foreground @min-[54rem]:block">
              {dateFormatter.format(trace.created_at * 1000)}
            </div>

            <div className="min-w-0">
              <div className="flex min-w-0 items-center justify-between gap-3 @min-[54rem]:block">
                <div className="flex min-w-0 items-center gap-2">
                  <span className="truncate text-sm font-semibold">{trace.requested_model}</span>
                  <span className="@min-[54rem]:hidden"><OutcomeBadge outcome={trace.outcome} /></span>
                </div>
                <span className="shrink-0 text-[0.6875rem] tabular-nums text-muted-foreground @min-[54rem]:hidden">
                  {dateFormatter.format(trace.created_at * 1000)}
                </span>
              </div>
              <div className="mt-1 truncate font-mono text-[0.6875rem] text-muted-foreground" title={trace.request_id}>{trace.request_id}</div>
            </div>

            <div className="hidden min-w-0 text-xs @min-[54rem]:block">
              <Chip className="max-w-full bg-info/10 text-info" size="sm" variant="flat">
                <span className="truncate">{t(`debugTraces.protocol.${trace.downstream_protocol}`)}</span>
              </Chip>
              <div className="mt-1 truncate text-[0.6875rem] text-muted-foreground">
                {converted ? t('debugTraces.values.convertedProtocol', { protocol: t(`debugTraces.protocol.${trace.upstream_protocol}`) }) : t(`debugTraces.operation.${trace.operation}`)}
              </div>
            </div>

            <div className="hidden text-xs tabular-nums @min-[54rem]:block">
              <div>{t('debugTraces.values.attemptsShort', { count: trace.attempt_count })}</div>
              <div className="mt-1 text-[0.6875rem] text-muted-foreground">{t('debugTraces.values.elapsed', { value: trace.routing_elapsed_ms })}</div>
            </div>

            <div className="hidden min-w-0 text-xs @min-[54rem]:block">
              <div className="truncate">{t('debugTraces.values.user', { id: trace.user_id })}</div>
              <div className="mt-1 truncate text-[0.6875rem] text-muted-foreground">{t('debugTraces.values.token', { id: trace.token_id })}</div>
            </div>

            <div className="hidden @min-[54rem]:block"><OutcomeBadge outcome={trace.outcome} /></div>
            <ChevronRight className="hidden size-4 text-muted-foreground transition-transform group-hover:translate-x-0.5 @min-[54rem]:block" aria-hidden="true" />

            <div className="mt-1 grid grid-cols-3 divide-x divide-[var(--hairline)] border-t border-[var(--hairline)] pt-3 text-[0.6875rem] @min-[54rem]:hidden">
              <div className="min-w-0 pr-2">
                <span className="flex items-center gap-1 text-muted-foreground"><Route className="size-3" aria-hidden="true" />{t('debugTraces.columns.protocol')}</span>
                <span className="mt-1 block truncate font-medium">{t(`debugTraces.protocol.${trace.downstream_protocol}`)}</span>
              </div>
              <div className="min-w-0 px-2">
                <span className="flex items-center gap-1 text-muted-foreground"><Clock3 className="size-3" aria-hidden="true" />{t('debugTraces.columns.routing')}</span>
                <span className="mt-1 block truncate font-medium tabular-nums">{t('debugTraces.values.routingCompact', { count: trace.attempt_count, value: trace.routing_elapsed_ms })}</span>
              </div>
              <div className="min-w-0 pl-2">
                <span className="text-muted-foreground">{t('debugTraces.columns.principal')}</span>
                <span className="mt-1 block truncate font-medium">{t('debugTraces.values.principalCompact', { user: trace.user_id, token: trace.token_id })}</span>
              </div>
            </div>
          </button>
        )
      })}
    </section>
  )
}

export function OutcomeBadge({ outcome }: { outcome: string }) {
  const { t } = useTranslation()
  const succeeded = outcome === 'succeeded'
  return (
    <Chip className={cn(succeeded ? 'bg-success/10 text-success' : 'bg-destructive/10 text-destructive')} size="sm" variant="flat">
      {t(`debugTraces.outcome.${succeeded ? 'succeeded' : 'failed'}`)}
    </Chip>
  )
}
