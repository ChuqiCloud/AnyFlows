import { Button, Chip, Input, Select, SelectItem, Textarea } from '@heroui/react'
import { RefreshCw, Send } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import type { ModelCatalogItem } from '@/lib/api/generated/types.gen'
import type { VideoTaskRequest } from './video-task-types'

const ASPECT_RATIO_ITEMS = ['1:1', '16:9', '9:16', '4:3', '3:4', '3:2', '2:3'] as const
const RESOLUTION_ITEMS = ['480p', '720p', '1080p'] as const
// HeroUI Select 不接受空字符串 key，用哨兵 key 表达“跟随上游默认值”。
const UPSTREAM_DEFAULT_KEY = '__default__'

type VideoTaskRequestPanelProps = {
  busy: boolean
  credentialReady: boolean
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
        <Chip className="bg-surface-2 font-mono text-[0.625rem] text-muted-foreground" size="sm" variant="flat">
          xAI Video
        </Chip>
      </div>

      <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground" htmlFor="video-task-model">
        <span>{t('videoTasks.request.model')}</span>
        <Input
          id="video-task-model"
          isDisabled={props.locked}
          list="video-task-model-options"
          maxLength={256}
          placeholder={t('videoTasks.request.modelPlaceholder')}
          size="sm"
          value={model}
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
              isIconOnly
              type="button"
              size="sm"
              className="size-6 min-w-6"
              variant="light"
              title={t('videoTasks.models.refresh')}
              isDisabled={props.loadingModels || props.locked}
              onClick={props.onRefreshModels}
            >
              <RefreshCw className={cn('size-3', props.loadingModels && 'animate-spin')} aria-hidden="true" />
              <span className="sr-only">{t('videoTasks.models.refresh')}</span>
            </Button>
            {props.hasMore ? (
              <Button
                type="button"
                size="sm"
                variant="light"
                isDisabled={props.loadingMore || props.locked}
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
          className="min-h-28 resize-y"
          isDisabled={props.locked}
          maxLength={32_768}
          minRows={4}
          placeholder={t('videoTasks.request.promptPlaceholder')}
          size="sm"
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
        />
      </label>

      <div className="grid gap-2 sm:grid-cols-3">
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('videoTasks.request.duration')}</span>
          <Select
            aria-label={t('videoTasks.request.duration')}
            isDisabled={props.locked}
            items={[
              { key: UPSTREAM_DEFAULT_KEY, label: t('videoTasks.request.upstreamDefault') },
              ...Array.from({ length: 15 }, (_, index) => index + 1).map((seconds) => ({
                key: String(seconds),
                label: t('videoTasks.request.seconds', { count: seconds }),
              })),
            ]}
            selectedKeys={[duration === '' ? UPSTREAM_DEFAULT_KEY : duration]}
            size="sm"
            onSelectionChange={(keys) => {
              const next = String(Array.from(keys)[0] ?? UPSTREAM_DEFAULT_KEY)
              setDuration(next === UPSTREAM_DEFAULT_KEY ? '' : next)
            }}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
        </label>
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('videoTasks.request.aspectRatio')}</span>
          <Select
            aria-label={t('videoTasks.request.aspectRatio')}
            isDisabled={props.locked}
            items={[
              { key: UPSTREAM_DEFAULT_KEY, label: t('videoTasks.request.upstreamDefault') },
              ...ASPECT_RATIO_ITEMS.map((value) => ({ key: value, label: value })),
            ]}
            selectedKeys={[aspectRatio === '' ? UPSTREAM_DEFAULT_KEY : aspectRatio]}
            size="sm"
            onSelectionChange={(keys) => {
              const next = String(Array.from(keys)[0] ?? UPSTREAM_DEFAULT_KEY)
              setAspectRatio(next === UPSTREAM_DEFAULT_KEY ? '' : next)
            }}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
        </label>
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('videoTasks.request.resolution')}</span>
          <Select
            aria-label={t('videoTasks.request.resolution')}
            isDisabled={props.locked}
            items={[
              { key: UPSTREAM_DEFAULT_KEY, label: t('videoTasks.request.upstreamDefault') },
              ...RESOLUTION_ITEMS.map((value) => ({ key: value, label: value })),
            ]}
            selectedKeys={[resolution === '' ? UPSTREAM_DEFAULT_KEY : resolution]}
            size="sm"
            onSelectionChange={(keys) => {
              const next = String(Array.from(keys)[0] ?? UPSTREAM_DEFAULT_KEY)
              setResolution(next === UPSTREAM_DEFAULT_KEY ? '' : next)
            }}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
        </label>
      </div>

      {validationError ? <p role="alert" className="text-xs text-destructive">{validationError}</p> : null}

      <Button type="submit" color="primary" size="sm" isDisabled={!props.credentialReady || props.busy || props.locked}>
        <Send className="size-3.5" aria-hidden="true" />
        {t(props.busy ? 'videoTasks.actions.submitting' : 'videoTasks.actions.submit')}
      </Button>
    </form>
  )
}
