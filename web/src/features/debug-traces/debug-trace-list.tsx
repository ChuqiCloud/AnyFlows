import { Activity, Clock3, Route } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import type { AdminDebugTrace } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type DebugTraceListProps = {
  traces: readonly AdminDebugTrace[]
  selectedId?: number
  onSelect: (traceId: number) => void
}

/** 以紧凑可扫描的请求摘要呈现主列表，避免数据表在窄屏横向溢出。 */
export function DebugTraceList({ traces, selectedId, onSelect }: DebugTraceListProps) {
  const { t, i18n } = useTranslation()
  if (traces.length === 0) {
    return (
      <div className="grid min-h-72 place-items-center px-6 py-10 text-center">
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
    <div className="divide-y divide-[var(--hairline)]">
      {traces.map((trace) => {
        const selected = selectedId === trace.id
        return (
          <button
            key={trace.id}
            type="button"
            aria-pressed={selected}
            className={cn(
              'grid w-full gap-2 px-3 py-3 text-left outline-none transition-colors',
              'hover:bg-surface-2/45 focus-visible:bg-surface-2 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50',
              selected && 'bg-brand/7 shadow-[inset_2px_0_0_var(--brand)]',
            )}
            onClick={() => onSelect(trace.id)}
          >
            <div className="flex min-w-0 items-center justify-between gap-3">
              <div className="flex min-w-0 items-center gap-2">
                <OutcomeBadge outcome={trace.outcome} />
                <span className="truncate text-sm font-semibold">{trace.requested_model}</span>
              </div>
              <span className="shrink-0 text-[0.6875rem] tabular-nums text-muted-foreground">
                {dateFormatter.format(trace.created_at * 1000)}
              </span>
            </div>
            <div className="truncate font-mono text-[0.6875rem] text-muted-foreground" title={trace.request_id}>
              {trace.request_id}
            </div>
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[0.6875rem] text-muted-foreground">
              <span className="inline-flex items-center gap-1"><Route className="size-3" aria-hidden="true" />{t('debugTraces.values.attempts', { count: trace.attempt_count })}</span>
              <span className="inline-flex items-center gap-1 tabular-nums"><Clock3 className="size-3" aria-hidden="true" />{t('debugTraces.values.elapsed', { value: trace.routing_elapsed_ms })}</span>
              <span>{t('debugTraces.values.principal', { user: trace.user_id, token: trace.token_id })}</span>
            </div>
          </button>
        )
      })}
    </div>
  )
}

export function OutcomeBadge({ outcome }: { outcome: string }) {
  const { t } = useTranslation()
  const succeeded = outcome === 'succeeded'
  return (
    <Badge className={cn('border-transparent', succeeded ? 'bg-success/10 text-success' : 'bg-destructive/10 text-destructive')}>
      {t(`debugTraces.outcome.${succeeded ? 'succeeded' : 'failed'}`)}
    </Badge>
  )
}
