import { ScanSearch } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Select } from '@/components/ui/select'
import type { AdminChannel } from '@/lib/api/generated/types.gen'

type ModelSyncToolbarProps = {
  channelId?: number
  channels: AdminChannel[]
  error: boolean
  hasMore: boolean
  loading: boolean
  loadingMore: boolean
  moreError: boolean
  onChannelChange: (channelId?: number) => void
  onLoadMore: () => void
  onPreview: () => void
  onRetry: () => void
  onRetryMore: () => void
  previewing: boolean
}

export function ModelSyncToolbar(props: ModelSyncToolbarProps) {
  const { t } = useTranslation()
  const selected = props.channels.find((channel) => channel.id === props.channelId)

  return (
    <div className="grid gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
        <label className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span>{t('modelManagement.sync.channel.label')}</span>
          <Select
            value={props.channelId ?? ''}
            disabled={props.loading || props.error}
            onChange={(event) => props.onChannelChange(event.target.value ? Number(event.target.value) : undefined)}
          >
            <option value="">{t(props.loading ? 'modelManagement.sync.channel.loading' : 'modelManagement.sync.channel.placeholder')}</option>
            {props.channels.map((channel) => (
              <option key={channel.id} value={channel.id} disabled={channel.status !== 'enabled'}>
                {t('modelManagement.sync.channel.option', {
                  name: channel.name,
                  protocol: t(`channels.protocol.${channel.protocol}`),
                  status: t(`channels.status.${channel.status}`),
                })}
              </option>
            ))}
          </Select>
        </label>
        <Button type="button" size="sm" disabled={!selected || selected.status !== 'enabled' || props.previewing} onClick={props.onPreview}>
          {props.previewing ? null : <ScanSearch aria-hidden="true" />}
          {t(props.previewing ? 'modelManagement.sync.actions.previewing' : 'modelManagement.sync.actions.preview')}
        </Button>
      </div>

      {props.previewing ? (
        <AiActivity
          active
          className="rounded-lg border border-info/15 bg-info/6 px-2.5 py-2"
          detail={selected?.name}
          label={t('modelManagement.sync.actions.previewing')}
          size="compact"
        />
      ) : null}

      {selected ? (
        <div className="flex flex-wrap gap-1.5">
          <Badge>{t(`channels.protocol.${selected.protocol}`)}</Badge>
          <Badge>{t(`channels.type.${selected.type}`)}</Badge>
          <Badge>{selected.timeout_secs === null ? t('channels.values.inheritTimeout') : t('channels.values.timeout', { value: selected.timeout_secs })}</Badge>
        </div>
      ) : null}

      {props.error ? (
        <div role="alert" className="flex flex-wrap items-center justify-between gap-2 text-xs text-destructive">
          <span>{t('modelManagement.sync.channel.error')}</span>
          <Button type="button" size="xs" variant="ghost" onClick={props.onRetry}>{t('modelManagement.actions.retry')}</Button>
        </div>
      ) : null}
      {props.moreError ? (
        <div role="alert" className="flex flex-wrap items-center justify-between gap-2 text-xs text-destructive">
          <span>{t('modelManagement.sync.channel.moreError')}</span>
          <Button type="button" size="xs" variant="ghost" onClick={props.onRetryMore}>{t('modelManagement.actions.retry')}</Button>
        </div>
      ) : null}
      {props.hasMore && !props.moreError ? (
        <Button type="button" size="xs" variant="ghost" className="justify-self-start" disabled={props.loadingMore} onClick={props.onLoadMore}>
          {t(props.loadingMore ? 'modelManagement.loadingMore' : 'modelManagement.sync.channel.loadMore')}
        </Button>
      ) : null}
    </div>
  )
}
