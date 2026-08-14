import { CheckCircle2, ChevronRight, Clock3, RotateCcw, Route, XCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminDebugTraceDetailResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { OutcomeBadge } from './debug-trace-list'
import { DebugTraceSnapshotPanel } from './debug-trace-snapshot'

type DebugTraceDetailProps = {
  detail?: AdminDebugTraceDetailResponse
  pending: boolean
  error: boolean
  onRetry: () => void
}

type TraceAttempt = AdminDebugTraceDetailResponse['attempts'][number]

/** 呈现请求概览、下游安全快照和可展开的上游 Attempt 诊断。 */
export function DebugTraceDetail({ detail, pending, error, onRetry }: DebugTraceDetailProps) {
  const { t, i18n } = useTranslation()
  if (pending) {
    return <div className="grid gap-3 p-4" aria-label={t('debugTraces.detail.loading')}>{[0, 1, 2, 3].map((item) => <Skeleton key={item} className={item === 0 ? 'h-24 rounded-lg' : 'h-16 rounded-lg'} />)}</div>
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

  const { trace, attempts, downstream_request: downstream } = detail
  const createdAt = new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium', timeStyle: 'medium' }).format(trace.created_at * 1000)
  return (
    <div className="min-w-0">
      <header className="sticky top-0 border-b border-[var(--hairline)] bg-popover/95 p-5 pr-12 supports-backdrop-filter:backdrop-blur-md">
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

      <div className="grid gap-5 p-5">
        <details className="group border border-[var(--hairline)] bg-surface-1/30" open>
          <summary className="flex min-h-11 cursor-pointer list-none items-center gap-2 px-3 outline-none hover:bg-surface-2/40 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50">
            <ChevronRight className="size-3.5 shrink-0 transition-transform group-open:rotate-90" aria-hidden="true" />
            <span className="text-xs font-semibold">{t('debugTraces.detail.downstreamRequest')}</span>
            {downstream ? <span className="ml-auto min-w-0 truncate font-mono text-[0.6875rem] text-muted-foreground">{downstream.method} {downstream.path}</span> : <span className="ml-auto text-[0.6875rem] text-muted-foreground">{t('debugTraces.detail.notCaptured')}</span>}
          </summary>
          <div className="grid gap-2 border-t border-[var(--hairline)] p-2 md:grid-cols-2">
            <DebugTraceSnapshotPanel title={t('debugTraces.detail.requestHeaders')} value={downstream?.headers} />
            <DebugTraceSnapshotPanel title={t('debugTraces.detail.requestBody')} value={downstream?.body} />
          </div>
        </details>

        <section>
          <div className="mb-2 flex items-center justify-between gap-3">
            <h4 className="text-xs font-semibold">{t('debugTraces.detail.attempts')}</h4>
            <span className="text-[0.6875rem] text-muted-foreground">{t('debugTraces.values.attempts', { count: attempts.length })}</span>
          </div>
          {attempts.length === 0 ? <p className="border border-dashed border-[var(--hairline)] p-5 text-center text-xs text-muted-foreground">{t('debugTraces.detail.noAttempts')}</p> : (
            <div className="border border-[var(--hairline)]">
              {attempts.map((attempt) => <AttemptRow key={attempt.candidate_index} attempt={attempt} />)}
            </div>
          )}
        </section>
      </div>
    </div>
  )
}

function AttemptRow({ attempt }: { attempt: TraceAttempt }) {
  const { t } = useTranslation()
  const succeeded = attempt.outcome === 'succeeded'
  return (
    <details className="group border-b border-[var(--hairline)] last:border-b-0">
      <summary className="grid min-h-12 cursor-pointer list-none grid-cols-[auto_auto_minmax(0,1fr)_auto] items-center gap-2 px-3 outline-none hover:bg-surface-2/40 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50">
        <ChevronRight className="size-3.5 transition-transform group-open:rotate-90" aria-hidden="true" />
        <span className={cn('grid size-5 place-items-center', succeeded ? 'text-success' : 'text-destructive')}>
          {succeeded ? <CheckCircle2 className="size-4" aria-hidden="true" /> : <XCircle className="size-4" aria-hidden="true" />}
        </span>
        <span className="min-w-0">
          <span className="flex min-w-0 items-center gap-2">
            <span className="shrink-0 text-xs font-semibold">{t('debugTraces.detail.candidate', { index: attempt.candidate_index + 1 })}</span>
            <span className="truncate font-mono text-[0.6875rem] text-muted-foreground">{attempt.request_method ?? ''} {attempt.request_url ?? t('debugTraces.detail.notCaptured')}</span>
          </span>
          <span className="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[0.6875rem] text-muted-foreground">
            <span>{t('debugTraces.detail.channelCredential', { channel: attempt.channel_id, credential: attempt.credential_id })}</span>
            {attempt.failure_kind ? <span className="text-destructive">{t(`debugTraces.failure.${attempt.failure_kind}`)}</span> : null}
            {attempt.response_status ? <span>HTTP {attempt.response_status}</span> : attempt.upstream_status ? <span>HTTP {attempt.upstream_status}</span> : null}
            {attempt.retry_decision ? <span className="inline-flex items-center gap-1 text-warning"><RotateCcw className="size-3" aria-hidden="true" />{t('debugTraces.detail.retried')}</span> : null}
          </span>
        </span>
        <span className="inline-flex items-center gap-1 font-mono text-[0.6875rem] tabular-nums text-muted-foreground"><Clock3 className="size-3" aria-hidden="true" />{t('debugTraces.values.elapsed', { value: attempt.elapsed_ms })}</span>
      </summary>
      <div className="grid gap-2 border-t border-[var(--hairline)] bg-surface-2/20 p-2 md:grid-cols-2">
        <DebugTraceSnapshotPanel title={t('debugTraces.detail.requestHeaders')} value={attempt.request_headers} />
        <DebugTraceSnapshotPanel title={t('debugTraces.detail.requestBody')} value={attempt.request_body} />
        <DebugTraceSnapshotPanel title={t('debugTraces.detail.responseHeaders')} value={attempt.response_headers} />
        <DebugTraceSnapshotPanel title={t('debugTraces.detail.responseBody')} value={attempt.response_body} streamed={attempt.response_streamed} />
      </div>
    </details>
  )
}

function Meta({ label, value }: { label: string; value: string }) {
  return <div className="min-w-0"><dt className="text-[0.6875rem] text-muted-foreground">{label}</dt><dd className="mt-0.5 truncate font-medium tabular-nums" title={value}>{value}</dd></div>
}
