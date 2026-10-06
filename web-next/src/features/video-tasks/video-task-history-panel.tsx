import { Button, Chip, Skeleton } from '@heroui/react'
import { Clock3, Film, History, RefreshCw, TriangleAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import type { VideoTaskApiError, VideoTaskListItem, VideoTaskStatus } from './video-task-types'

type VideoTaskHistoryPanelProps = {
  activeTaskId?: string
  error?: VideoTaskApiError
  items: VideoTaskListItem[]
  phase: 'idle' | 'loading' | 'ready' | 'loadingMore' | 'error'
  hasMore: boolean
  onLoadMore: () => void
  onOpen: (taskId: string) => void
  onRefresh: () => void
}

/** 展示 owner-scoped 持久化摘要，选中后才刷新单任务状态与短期结果。 */
export function VideoTaskHistoryPanel(props: VideoTaskHistoryPanelProps) {
  const { t, i18n } = useTranslation()
  const busy = props.phase === 'loading' || props.phase === 'loadingMore'

  return (
    <section>
      <header className="flex min-h-12 items-center justify-between gap-3 border-b border-[var(--hairline)] px-3 py-2">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <History className="size-3.5 text-brand" aria-hidden="true" />
            <h3 className="text-xs font-semibold">{t('videoTasks.history.title')}</h3>
            {props.items.length > 0 ? <Chip className="bg-surface-2 text-muted-foreground" size="sm" variant="flat">{props.items.length}</Chip> : null}
          </div>
          <p className="mt-0.5 truncate text-[0.6875rem] text-muted-foreground">
            {t('videoTasks.history.description')}
          </p>
        </div>
        <Button
          isIconOnly
          type="button"
          size="sm"
          className="size-6 min-w-6"
          variant="light"
          isDisabled={busy || props.phase === 'idle'}
          title={t('videoTasks.history.refresh')}
          onClick={props.onRefresh}
        >
          <RefreshCw className={cn('size-3', props.phase === 'loading' && 'animate-spin')} aria-hidden="true" />
          <span className="sr-only">{t('videoTasks.history.refresh')}</span>
        </Button>
      </header>

      {props.phase === 'idle' ? <HistoryMessage icon="key" text={t('videoTasks.history.apiKey')} /> : null}
      {props.phase === 'loading' ? <HistorySkeleton /> : null}
      {props.phase === 'error' && props.error ? (
        <HistoryMessage
          icon="error"
          text={t(`videoTasks.error.codes.${props.error.code}`, { defaultValue: props.error.message })}
        />
      ) : null}
      {props.phase === 'ready' && props.items.length === 0
        ? <HistoryMessage icon="empty" text={t('videoTasks.history.empty')} />
        : null}
      {props.items.length > 0 ? (
        <div className="divide-y divide-[var(--hairline)]">
          {props.items.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`grid w-full grid-cols-[minmax(0,1fr)_auto] gap-3 px-3 py-2.5 text-left transition-colors hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-brand ${item.id === props.activeTaskId ? 'bg-surface-2' : ''}`}
              aria-current={item.id === props.activeTaskId ? 'true' : undefined}
              onClick={() => props.onOpen(item.id)}
            >
              <span className="min-w-0">
                <span className="block truncate text-xs font-medium">{item.model}</span>
                <span className="mt-1 flex items-center gap-1.5 font-mono text-[0.625rem] text-muted-foreground">
                  <Clock3 className="size-3" aria-hidden="true" />
                  {formatTaskTime(item.updated_at, i18n.language)}
                  <span aria-hidden="true">·</span>
                  {item.id.slice(0, 8)}
                </span>
              </span>
              <StatusBadge status={item.status} progress={item.progress_basis_points} />
            </button>
          ))}
        </div>
      ) : null}

      {props.hasMore ? (
        <div className="border-t border-[var(--hairline)] p-2">
          <Button
            type="button"
            size="sm"
            variant="light"
            className="w-full"
            isDisabled={props.phase !== 'ready'}
            onClick={props.onLoadMore}
          >
            <RefreshCw className={cn('size-3.5', props.phase === 'loadingMore' && 'animate-spin')} aria-hidden="true" />
            {t(props.phase === 'loadingMore' ? 'videoTasks.history.loadingMore' : 'videoTasks.history.loadMore')}
          </Button>
        </div>
      ) : null}
    </section>
  )
}

function StatusBadge({ status, progress }: { status: VideoTaskStatus; progress: number }) {
  const { t } = useTranslation()
  const className = status === 'done'
    ? 'bg-success/12 text-success'
    : status === 'failed' || status === 'expired'
      ? 'bg-destructive/10 text-destructive'
      : 'bg-warning/12 text-warning'
  const label = status === 'pending' && progress > 0
    ? `${Math.floor(progress / 100)}%`
    : t(`videoTasks.status.${status}`)
  return <Chip className={className} size="sm" variant="flat">{label}</Chip>
}

function HistorySkeleton() {
  return (
    <div className="grid gap-3 p-3" aria-hidden="true">
      {[0, 1, 2].map((item) => <Skeleton key={item} className="h-10 w-full rounded-md" />)}
    </div>
  )
}

function HistoryMessage({ icon, text }: { icon: 'key' | 'empty' | 'error'; text: string }) {
  const Icon = icon === 'error' ? TriangleAlert : Film
  return (
    <div className="grid min-h-28 place-items-center px-4 py-6 text-center text-xs leading-5 text-muted-foreground">
      <span>
        <Icon className="mx-auto mb-2 size-5 opacity-60" aria-hidden="true" />
        {text}
      </span>
    </div>
  )
}

function formatTaskTime(timestamp: number, language: string) {
  return new Intl.DateTimeFormat(language, {
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(timestamp * 1_000))
}
