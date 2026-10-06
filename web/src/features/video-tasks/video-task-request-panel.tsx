import { RefreshCw, Send } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import type { ModelCatalogItem } from '@/lib/api/generated/types.gen'
import type { VideoTaskRequest } from './video-task-types'

type VideoTaskRequestPanelProps = {
  busy: boolean
  credentialReady: boolean
  idempotencyKey: string
  locked: boolean
  models: ModelCatalogItem[]
  loadingModels: boolean
  loadingMore: boolean
  hasMore: boolean
  modelError: boolean
  onLoadMore: () => void
  onRefreshModels: () => void
  onSubmit: (request: VideoTaskRequest) => void
}

/** 用闭合选择控件构造视频任务请求，避免用户手写协议 JSON。 */
export function VideoTaskRequestPanel(props: VideoTaskRequestPanelProps) {
  const { t } = useTranslation()
  const [model, setModel] = useState('')
  const [prompt, setPrompt] = useState('')
  const [duration, setDuration] = useState('')
  const [aspectRatio, setAspectRatio] = useState('')
  const [resolution, setResolution] = useState('')
  const [validationError, setValidationError] = useState<string>()

  useEffect(() => {
    if (!model && props.models[0]) setModel(props.models[0].model)
  }, [model, props.models])

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const normalizedModel = model.trim()
    const normalizedPrompt = prompt.trim()
    if (!normalizedModel || !normalizedPrompt) {
      setValidationError(t('videoTasks.request.required'))
      return
    }
    setValidationError(undefined)
    props.onSubmit({
      model: normalizedModel,
      prompt: normalizedPrompt,
      duration: duration ? Number(duration) : undefined,
      aspect_ratio: aspectRatio
        ? aspectRatio as VideoTaskRequest['aspect_ratio']
        : undefined,
      resolution: resolution
        ? resolution as VideoTaskRequest['resolution']
        : undefined,
    })
  }

  return (
    <form className="grid gap-4 border-t border-[var(--hairline)] p-3" onSubmit={submit}>
      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-xs font-semibold">{t('videoTasks.request.title')}</h3>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
            {t('videoTasks.request.description')}
          </p>
        </div>
        <Badge className="bg-surface-2 font-mono text-[0.625rem] text-muted-foreground">
          xAI Video
        </Badge>
      </div>

      <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground" htmlFor="video-task-model">
        <span>{t('videoTasks.request.model')}</span>
        <Input
          id="video-task-model"
          list="video-task-model-options"
          value={model}
          disabled={props.locked}
          maxLength={256}
          placeholder={t('videoTasks.request.modelPlaceholder')}
          onChange={(event) => setModel(event.target.value)}
        />
        <datalist id="video-task-model-options">
          {props.models.map((item) => <option key={item.model} value={item.model} />)}
        </datalist>
        <div className="flex items-center justify-between gap-2 text-[0.625rem] text-muted-foreground">
          <span>
            {props.loadingModels
              ? t('videoTasks.models.loading')
              : props.modelError
                ? t('videoTasks.models.error')
                : t('videoTasks.models.count', { count: props.models.length })}
          </span>
          <span className="flex items-center gap-1">
            <Button
              type="button"
              size="icon-xs"
              variant="ghost"
              title={t('videoTasks.models.refresh')}
              disabled={props.loadingModels || props.locked}
              onClick={props.onRefreshModels}
            >
              <RefreshCw className={props.loadingModels ? 'animate-spin' : undefined} aria-hidden="true" />
              <span className="sr-only">{t('videoTasks.models.refresh')}</span>
            </Button>
            {props.hasMore ? (
              <Button
                type="button"
                size="xs"
                variant="ghost"
                disabled={props.loadingMore || props.locked}
                onClick={props.onLoadMore}
              >
                {t('videoTasks.models.loadMore')}
              </Button>
            ) : null}
          </span>
        </div>
      </label>

      <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground" htmlFor="video-task-prompt">
        <span>{t('videoTasks.request.prompt')}</span>
        <Textarea
          id="video-task-prompt"
          className="min-h-28 resize-y text-xs leading-5"
          value={prompt}
          disabled={props.locked}
          maxLength={32_768}
          placeholder={t('videoTasks.request.promptPlaceholder')}
          onChange={(event) => setPrompt(event.target.value)}
        />
      </label>

      <div className="grid gap-2 sm:grid-cols-3">
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('videoTasks.request.duration')}</span>
          <Select value={duration} disabled={props.locked} onChange={(event) => setDuration(event.target.value)}>
            <option value="">{t('videoTasks.request.upstreamDefault')}</option>
            {Array.from({ length: 15 }, (_, index) => index + 1).map((seconds) => (
              <option key={seconds} value={seconds}>{t('videoTasks.request.seconds', { count: seconds })}</option>
            ))}
          </Select>
        </label>
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('videoTasks.request.aspectRatio')}</span>
          <Select value={aspectRatio} disabled={props.locked} onChange={(event) => setAspectRatio(event.target.value)}>
            <option value="">{t('videoTasks.request.upstreamDefault')}</option>
            {['1:1', '16:9', '9:16', '4:3', '3:4', '3:2', '2:3'].map((value) => (
              <option key={value} value={value}>{value}</option>
            ))}
          </Select>
        </label>
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('videoTasks.request.resolution')}</span>
          <Select value={resolution} disabled={props.locked} onChange={(event) => setResolution(event.target.value)}>
            <option value="">{t('videoTasks.request.upstreamDefault')}</option>
            {['480p', '720p', '1080p'].map((value) => (
              <option key={value} value={value}>{value}</option>
            ))}
          </Select>
        </label>
      </div>

      <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground" htmlFor="video-task-idempotency-key">
        <span>{t('videoTasks.request.idempotencyKey')}</span>
        <Input
          id="video-task-idempotency-key"
          className="font-mono text-[0.6875rem]"
          value={props.idempotencyKey}
          readOnly
        />
        <span className="text-[0.625rem] leading-4">{t('videoTasks.request.idempotencyHint')}</span>
      </label>

      {validationError ? <p role="alert" className="text-xs text-destructive">{validationError}</p> : null}

      <Button type="submit" size="sm" disabled={!props.credentialReady || props.busy || props.locked}>
        <Send aria-hidden="true" />
        {t(props.busy ? 'videoTasks.actions.submitting' : 'videoTasks.actions.submit')}
      </Button>
    </form>
  )
}
