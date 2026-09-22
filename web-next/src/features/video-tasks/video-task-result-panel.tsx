import { Button, Chip, Skeleton } from '@heroui/react'
import { Copy, ExternalLink, Film, RefreshCw, RotateCcw, TriangleAlert } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
import type { VideoTaskApiError, VideoTaskPollResponse } from './video-task-types'
import type { VideoTaskWorkspacePhase } from './use-video-task-workspace'

type VideoTaskResultPanelProps = {
  busy: boolean
  canPoll: boolean
  canRetrySubmission: boolean
  error?: VideoTaskApiError
  phase: VideoTaskWorkspacePhase
  response?: VideoTaskPollResponse
  taskId?: string
  onNew: () => void
  onPoll: () => void
  onRetrySubmission: () => void
}

/** 展示公开任务状态和短期视频结果，不把签名 URL 写入浏览器持久化。 */
export function VideoTaskResultPanel(props: VideoTaskResultPanelProps) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState<'task' | 'url'>()
  const status = props.phase === 'polling' ? 'pending' : props.phase

  const copy = async (kind: 'task' | 'url', value: string) => {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(kind)
      window.setTimeout(() => setCopied((current) => current === kind ? undefined : current), 1500)
    } catch {
      setCopied(undefined)
    }
  }

  return (
    <section className="flex min-h-[32rem] min-w-0 flex-col overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1">
      <header className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-b border-dashed border-[var(--hairline)] px-4 py-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <Film className="size-4 text-brand" aria-hidden="true" />
            <h3 className="text-sm font-semibold">{t('videoTasks.result.title')}</h3>
            <StatusBadge phase={status} />
          </div>
          <p className="mt-1 text-xs text-muted-foreground">{t('videoTasks.result.description')}</p>
        </div>
        <div className="flex items-center gap-1.5">
          {props.canPoll ? (
            <Button type="button" size="sm" variant="bordered" isDisabled={props.busy} onClick={props.onPoll}>
              <RefreshCw className="size-3.5" aria-hidden="true" />
              {t('videoTasks.actions.refresh')}
            </Button>
          ) : null}
          <Button type="button" size="sm" variant="light" onClick={props.onNew}>
            <RotateCcw className="size-3.5" aria-hidden="true" />{t('videoTasks.actions.newTask')}
          </Button>
        </div>
      </header>

      {props.taskId ? (
        <div className="flex min-h-10 items-center gap-2 border-b border-[var(--hairline)] px-4 py-2 text-xs text-muted-foreground">
          <span className="shrink-0">{t('videoTasks.result.taskId')}</span>
          <code className="min-w-0 flex-1 truncate font-mono text-[0.6875rem] text-foreground">{props.taskId}</code>
          <Button
            isIconOnly
            type="button"
            size="sm"
            className="size-6 min-w-6"
            variant="light"
            title={t('videoTasks.actions.copyTaskId')}
            onClick={() => void copy('task', props.taskId ?? '')}
          >
            <Copy className="size-3" aria-hidden="true" />
            <span className="sr-only">{t('videoTasks.actions.copyTaskId')}</span>
          </Button>
          {copied === 'task' ? <span className="text-success">{t('videoTasks.actions.copied')}</span> : null}
        </div>
      ) : null}

      <div className="grid min-h-0 flex-1 place-items-center p-4 sm:p-6">
        {props.phase === 'idle' ? <IdleState /> : null}
        {props.phase === 'submitting' || props.phase === 'pending' || props.phase === 'polling'
          ? <PendingState polling={props.phase === 'polling'} />
          : null}
        {props.phase === 'done' && props.response?.video
          ? <DoneState response={props.response} copied={copied === 'url'} onCopy={copy} />
          : null}
        {props.phase === 'failed' || props.phase === 'expired'
          ? <TerminalFailureState phase={props.phase} response={props.response} />
          : null}
        {props.phase === 'error' && props.error ? (
          <ErrorState
            error={props.error}
            retrySubmission={props.canRetrySubmission}
            onRetrySubmission={props.onRetrySubmission}
          />
        ) : null}
      </div>
    </section>
  )
}

function StatusBadge({ phase }: { phase: VideoTaskWorkspacePhase }) {
  const { t } = useTranslation()
  const className = phase === 'done'
    ? 'bg-success/12 text-success'
    : phase === 'failed' || phase === 'expired' || phase === 'error'
      ? 'bg-destructive/10 text-destructive'
      : phase === 'idle'
        ? 'bg-surface-2 text-muted-foreground'
        : 'bg-warning/12 text-warning'
  return <Chip className={className} size="sm" variant="flat">{t(`videoTasks.status.${phase}`)}</Chip>
}

function IdleState() {
  const { t } = useTranslation()
  return (
    <div className="max-w-sm text-center">
      <Film className="mx-auto size-8 text-muted-foreground/60" aria-hidden="true" />
      <h4 className="mt-4 text-sm font-semibold">{t('videoTasks.empty.title')}</h4>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('videoTasks.empty.body')}</p>
    </div>
  )
}

function PendingState({ polling }: { polling: boolean }) {
  const { t } = useTranslation()
  return (
    <div className="w-full max-w-2xl">
      <div className="relative overflow-hidden rounded-xl border border-[var(--hairline)] bg-[var(--surface-sunken)]">
        <Skeleton className="aspect-video w-full rounded-none opacity-45" />
        <div className="absolute inset-0 grid place-items-center bg-background/24 backdrop-blur-[2px]">
          <AiActivity
            active
            className="max-w-[80%]"
            detail={t('videoTasks.pending.body')}
            label={t(polling ? 'videoTasks.pending.polling' : 'videoTasks.pending.title')}
            size="panel"
          />
        </div>
      </div>
    </div>
  )
}

function DoneState(props: {
  copied: boolean
  response: VideoTaskPollResponse
  onCopy: (kind: 'task' | 'url', value: string) => Promise<void>
}) {
  const { t } = useTranslation()
  const video = props.response.video
  if (!video) return null
  return (
    <div className="w-full max-w-3xl">
      <video
        className="aspect-video w-full rounded-lg bg-black object-contain"
        controls
        playsInline
        preload="metadata"
        src={video.url}
      >
        {t('videoTasks.result.videoUnsupported')}
      </video>
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <Chip className="bg-success/12 text-success" size="sm" variant="flat">{t('videoTasks.result.complete')}</Chip>
          <span>{props.response.model}</span>
          <span>{t('videoTasks.result.duration', { count: video.duration })}</span>
        </div>
        <div className="flex items-center gap-2">
          <Button type="button" size="sm" variant="bordered" onClick={() => void props.onCopy('url', video.url)}>
            <Copy className="size-3.5" aria-hidden="true" />{props.copied ? t('videoTasks.actions.copied') : t('videoTasks.actions.copyUrl')}
          </Button>
          <Button as="a" href={video.url} rel="noreferrer" size="sm" target="_blank" variant="light">
            <ExternalLink className="size-3.5" aria-hidden="true" />{t('videoTasks.actions.openVideo')}
          </Button>
        </div>
      </div>
      <p className="mt-3 text-[0.6875rem] leading-4 text-muted-foreground">
        {t('videoTasks.result.temporaryUrl')}
      </p>
    </div>
  )
}

function TerminalFailureState(props: {
  phase: 'failed' | 'expired'
  response?: VideoTaskPollResponse
}) {
  const { t } = useTranslation()
  return (
    <div className="max-w-md text-center">
      <TriangleAlert className="mx-auto size-8 text-destructive" aria-hidden="true" />
      <h4 className="mt-4 text-sm font-semibold">{t(`videoTasks.${props.phase}.title`)}</h4>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">
        {props.response?.error?.message ?? t(`videoTasks.${props.phase}.body`)}
      </p>
    </div>
  )
}

function ErrorState(props: {
  error: VideoTaskApiError
  retrySubmission: boolean
  onRetrySubmission: () => void
}) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="max-w-md text-center">
      <TriangleAlert className="mx-auto size-8 text-destructive" aria-hidden="true" />
      <h4 className="mt-4 text-sm font-semibold">{t('videoTasks.error.title')}</h4>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">
        {t(`videoTasks.error.codes.${props.error.code}`, { defaultValue: props.error.message })}
      </p>
      {props.retrySubmission ? (
        <Button type="button" size="sm" variant="bordered" className="mt-4" onClick={props.onRetrySubmission}>
          <RefreshCw className="size-3.5" aria-hidden="true" />{t('videoTasks.actions.retrySameKey')}
        </Button>
      ) : null}
    </div>
  )
}
