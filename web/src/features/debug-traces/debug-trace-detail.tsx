import { CheckCircle2, Clock3, RotateCcw, Route, XCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminDebugTraceDetailResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { OutcomeBadge } from './debug-trace-list'

type DebugTraceDetailProps = {
  detail?: AdminDebugTraceDetailResponse
  pending: boolean
  error: boolean
  onRetry: () => void
}

/** 呈现单次请求的脱敏协议摘要和候选决策时间线。 */
export function DebugTraceDetail({ detail, pending, error, onRetry }: DebugTraceDetailProps) {
  const { t, i18n } = useTranslation()
  if (pending) {
    return <div className="grid gap-3 p-4" aria-label={t('debugTraces.detail.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className={item === 0 ? 'h-24 rounded-lg' : 'h-20 rounded-lg'} />)}</div>
  }
  if (error) {
    return (
      <div className="m-4 rounded-lg border border-destructive/25 bg-destructive/8 p-4" role="alert">
        <h3 className="text-sm font-semibold text-destructive">{t('debugTraces.detail.errorTitle')}</h3>
        <p className="mt-1 text-xs text-muted-foreground">{t('debugTraces.detail.errorBody')}</p>
        <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={onRetry}>{t('debugTraces.actions.retry')}</Button>
      </div>
    )
  }
  if (!detail) {
    return (
      <div className="grid min-h-96 place-items-center p-8 text-center">
        <div className="max-w-xs">
          <Route className="mx-auto size-6 text-muted-foreground" aria-hidden="true" />
          <h3 className="mt-3 text-sm font-semibold">{t('debugTraces.detail.selectTitle')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('debugTraces.detail.selectBody')}</p>
        </div>
      </div>
    )
  }

  const { trace, attempts } = detail
  const createdAt = new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium', timeStyle: 'medium' }).format(trace.created_at * 1000)
  return (
    <div>
      <header className="border-b border-[var(--hairline)] p-4">
        <div className="flex flex-wrap items-center gap-2">
          <OutcomeBadge outcome={trace.outcome} />
          <h3 className="text-sm font-semibold">{trace.requested_model}</h3>
          <Badge>{t(`debugTraces.protocol.${trace.downstream_protocol}`)}</Badge>
          {trace.downstream_protocol !== trace.upstream_protocol ? <Badge className="border-info/20 bg-info/8 text-info">{t('debugTraces.detail.convertedTo', { protocol: t(`debugTraces.protocol.${trace.upstream_protocol}`) })}</Badge> : null}
        </div>
        <p className="mt-2 break-all font-mono text-[0.6875rem] text-muted-foreground">{trace.request_id}</p>
        <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-2 text-xs sm:grid-cols-4">
          <Meta label={t('debugTraces.detail.createdAt')} value={createdAt} />
          <Meta label={t('debugTraces.detail.operation')} value={t(`debugTraces.operation.${trace.operation}`)} />
          <Meta label={t('debugTraces.detail.routingElapsed')} value={t('debugTraces.values.elapsed', { value: trace.routing_elapsed_ms })} />
          <Meta label={t('debugTraces.detail.group')} value={`#${trace.group_id}`} />
        </dl>
      </header>

      <div className="p-4">
        <div className="mb-3 flex items-center justify-between gap-3">
          <h4 className="text-xs font-semibold">{t('debugTraces.detail.timeline')}</h4>
          <span className="text-[0.6875rem] text-muted-foreground">{t('debugTraces.values.attempts', { count: attempts.length })}</span>
        </div>
        {attempts.length === 0 ? <p className="rounded-lg border border-dashed border-[var(--hairline)] p-5 text-center text-xs text-muted-foreground">{t('debugTraces.detail.noAttempts')}</p> : (
          <ol className="relative ml-3 border-l border-[var(--hairline)]">
            {attempts.map((attempt) => {
              const succeeded = attempt.outcome === 'succeeded'
              return (
                <li key={attempt.candidate_index} className="relative pb-5 pl-6 last:pb-0">
                  <span className={cn('absolute -left-3 grid size-6 place-items-center rounded-full border bg-background', succeeded ? 'border-success/35 text-success' : 'border-destructive/30 text-destructive')}>
                    {succeeded ? <CheckCircle2 className="size-3.5" aria-hidden="true" /> : <XCircle className="size-3.5" aria-hidden="true" />}
                  </span>
                  <div className="rounded-lg border border-[var(--hairline)] bg-surface-1/60 p-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-xs font-semibold">{t('debugTraces.detail.candidate', { index: attempt.candidate_index + 1 })}</span>
                        <Badge className={cn('border-transparent', succeeded ? 'bg-success/10 text-success' : 'bg-destructive/10 text-destructive')}>{t(`debugTraces.outcome.${attempt.outcome}`)}</Badge>
                        {attempt.retry_decision ? <Badge className="border-warning/20 bg-warning/8 text-warning"><RotateCcw aria-hidden="true" />{t('debugTraces.detail.retried')}</Badge> : null}
                      </div>
                      <span className="inline-flex items-center gap-1 text-[0.6875rem] tabular-nums text-muted-foreground"><Clock3 className="size-3" aria-hidden="true" />{t('debugTraces.values.elapsed', { value: attempt.elapsed_ms })}</span>
                    </div>
                    <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-[0.6875rem] text-muted-foreground">
                      <span>{t('debugTraces.detail.channelCredential', { channel: attempt.channel_id, credential: attempt.credential_id })}</span>
                      {attempt.failure_kind ? <span className="text-destructive">{t(`debugTraces.failure.${attempt.failure_kind}`)}</span> : null}
                      {attempt.upstream_status ? <span>HTTP {attempt.upstream_status}</span> : null}
                    </div>
                  </div>
                </li>
              )
            })}
          </ol>
        )}
      </div>
    </div>
  )
}

function Meta({ label, value }: { label: string; value: string }) {
  return <div><dt className="text-[0.6875rem] text-muted-foreground">{label}</dt><dd className="mt-0.5 truncate font-medium tabular-nums" title={value}>{value}</dd></div>
}
