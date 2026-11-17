import { useState } from 'react'
import { Button } from '@heroui/react'
import { Braces } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { ApiError } from '@/lib/api'
import {
  type ChannelModelMappingPluginReviewCheck,
  useChannelModelMappingPluginReviewers,
  useReviewChannelModelMappingWithPlugin,
} from './channel-model-mapping-plugin-review-api'
import { PluginUiViewsRenderer } from '@/features/plugin-lifecycle/plugin-ui-views'

function reviewCheckStyle(status: ChannelModelMappingPluginReviewCheck['status']) {
  if (status === 'fail') return 'border-danger/25 bg-danger/5'
  if (status === 'warning') return 'border-warning/25 bg-warning/5'
  if (status === 'pass') return 'border-success/25 bg-success/5'
  return 'border-[var(--hairline)] bg-surface-2/40'
}

function isForbidden(error: unknown) {
  return error instanceof ApiError && error.status === 403
}

export function ChannelModelMappingPluginReviewPanel({ channels }: { channels: AdminChannel[] }) {
  const { t, i18n } = useTranslation()
  const [open, setOpen] = useState(false)
  const reviewers = useChannelModelMappingPluginReviewers(open)
  const review = useReviewChannelModelMappingWithPlugin()
  const [selectedChannelId, setSelectedChannelId] = useState('')
  const [selectedPluginId, setSelectedPluginId] = useState('')
  const availableChannels = channels
  const availablePlugins = reviewers.data?.plugins ?? []
  const channelId = availableChannels.some((channel) => String(channel.id) === selectedChannelId)
    ? selectedChannelId
    : String(availableChannels[0]?.id ?? '')
  const pluginId = availablePlugins.some((plugin) => plugin.plugin_id === selectedPluginId)
    ? selectedPluginId
    : availablePlugins[0]?.plugin_id ?? ''

  const runReview = async () => {
    if (!channelId || !pluginId) return
    try {
      await review.mutateAsync({ channelId: Number(channelId), pluginId })
    } catch {
      // Keep the selections so the same review can be retried.
    }
  }

  const result = review.data

  return <details className="rounded-lg border border-[var(--hairline)] bg-surface-1" onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-4 py-3 text-sm font-semibold [&::-webkit-details-marker]:hidden">
      <span className="inline-flex items-center gap-2"><Braces className="size-4 text-primary" aria-hidden="true" />{t('channels.modelMappingPluginReview.title')}</span>
      <span className="text-[11px] font-normal text-muted-foreground">{t('channels.modelMappingPluginReview.summary')}</span>
    </summary>
    <div className="grid gap-3 border-t border-[var(--hairline)] p-4">
      <p className="text-xs leading-5 text-muted-foreground">{t('channels.modelMappingPluginReview.description')}</p>

      {reviewers.isPending ? <p role="status" className="text-xs text-muted-foreground">{t('channels.modelMappingPluginReview.loadingPlugins')}</p> : null}
      {reviewers.isError ? <div role="alert" className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-danger/25 bg-danger/8 p-3 text-xs text-danger">
        <span>{isForbidden(reviewers.error) ? t('channels.modelMappingPluginReview.forbidden') : t('channels.modelMappingPluginReview.pluginsError')}</span>
        <Button size="sm" variant="flat" onPress={() => void reviewers.refetch()}>{t('channels.modelMappingPluginReview.retry')}</Button>
      </div> : null}

      {!reviewers.isPending && !reviewers.isError && availableChannels.length === 0 ? <p className="text-xs text-muted-foreground">{t('channels.modelMappingPluginReview.noChannels')}</p> : null}
      {!reviewers.isPending && !reviewers.isError && availableChannels.length > 0 && availablePlugins.length === 0 ? <p className="text-xs text-muted-foreground">{t('channels.modelMappingPluginReview.noPlugins')}</p> : null}

      {!reviewers.isPending && !reviewers.isError && availableChannels.length > 0 && availablePlugins.length > 0 ? <>
        <div className="flex flex-wrap items-end gap-2">
          <label className="grid min-w-48 flex-1 gap-1 text-xs">{t('channels.modelMappingPluginReview.channel')}
            <select className="h-9 min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-3" aria-label={t('channels.modelMappingPluginReview.channel')} value={channelId} disabled={review.isPending} onChange={(event) => { setSelectedChannelId(event.target.value); review.reset() }}>
              {availableChannels.map((channel) => <option key={channel.id} value={channel.id}>{channel.name} · #{channel.id}</option>)}
            </select>
          </label>
          <label className="grid min-w-48 flex-1 gap-1 text-xs">{t('channels.modelMappingPluginReview.plugin')}
            <select className="h-9 min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-3" aria-label={t('channels.modelMappingPluginReview.plugin')} value={pluginId} disabled={review.isPending} onChange={(event) => { setSelectedPluginId(event.target.value); review.reset() }}>
              {availablePlugins.map((plugin) => <option key={plugin.plugin_id} value={plugin.plugin_id}>{plugin.display_name} · {plugin.plugin_version}</option>)}
            </select>
          </label>
          <Button size="sm" color="primary" isDisabled={!channelId || !pluginId || review.isPending} isLoading={review.isPending} onPress={() => void runReview()}>{t('channels.modelMappingPluginReview.run')}</Button>
        </div>

        {review.isError ? <div role="alert" className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-danger/25 bg-danger/8 p-3 text-xs text-danger">
          <span>{isForbidden(review.error) ? t('channels.modelMappingPluginReview.forbidden') : t('channels.modelMappingPluginReview.runError')}</span>
          <Button size="sm" variant="flat" onPress={() => void runReview()}>{t('channels.modelMappingPluginReview.retry')}</Button>
        </div> : null}

        {result ? <div className="grid gap-2" aria-live="polite">
          <div className="rounded-md border border-[var(--hairline)] p-2">
            <div className="flex flex-wrap items-center justify-between gap-1 text-[10px] text-muted-foreground">
              <span>{result.display_name} · {result.plugin_version}</span>
              <span>{t('channels.modelMappingPluginReview.counts', { models: result.model_count, mappings: result.mapping_count })}</span>
            </div>
            <p className="mt-1 whitespace-pre-wrap break-words text-xs leading-5">{result.summary}</p>
          </div>
          {result.checks.length ? <ul className="grid gap-1.5">{result.checks.map((check, index) => <li key={`${check.code}-${index}`} className={`grid gap-1 rounded-md border p-2 ${reviewCheckStyle(check.status)}`}>
            <div className="flex flex-wrap items-center gap-2 text-[10px]">
              <span>{t(`channels.modelMappingPluginReview.status.${check.status}`, { defaultValue: check.status })}</span>
              <code className="break-all text-muted-foreground">{check.code}</code>
            </div>
            <strong className="text-[11px]">{check.title}</strong>
            <p className="whitespace-pre-wrap break-words text-[11px] leading-4">{check.message}</p>
            {check.recommendation ? <p className="whitespace-pre-wrap break-words text-[11px] leading-4 text-muted-foreground">{t('channels.modelMappingPluginReview.recommendation')}: {check.recommendation}</p> : null}
          </li>)}</ul> : <p className="text-[11px] text-muted-foreground">{t('channels.modelMappingPluginReview.noChecks')}</p>}
          <PluginUiViewsRenderer manifest={{ uiViews: result.ui_views }} slot="channel_model_mapping" locale={i18n.resolvedLanguage ?? 'en'} />
        </div> : null}
      </> : null}
    </div>
  </details>
}
