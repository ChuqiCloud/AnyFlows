import { Button, Chip, Select, SelectItem } from '@heroui/react'
import { ScanSearch } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
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

/** HeroUI Select 不接受空字符串 key，用哨兵 key 表达"未选择渠道"。 */
const PLACEHOLDER_CHANNEL_KEY = '__placeholder__'

export function ModelSyncToolbar(props: ModelSyncToolbarProps) {
  const { t } = useTranslation()
  const selected = props.channels.find((channel) => channel.id === props.channelId)
  // HeroUI Select 的动态选项必须走 items + 渲染函数；空字符串 key 用哨兵表达。
  const channelItems = [
    { key: PLACEHOLDER_CHANNEL_KEY, label: t(props.loading ? 'modelManagement.sync.channel.loading' : 'modelManagement.sync.channel.placeholder'), disabled: false },
    ...props.channels.map((channel) => ({
      key: String(channel.id),
      disabled: channel.status !== 'enabled',
      label: t('modelManagement.sync.channel.option', {
        name: channel.name,
        protocol: t(`channels.protocol.${channel.protocol}`),
        status: t(`channels.status.${channel.status}`),
      }),
    })),
  ]

  return (
    <div className="grid gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
      <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
        <div className="grid gap-1.5 text-[0.6875rem] font-medium text-muted-foreground">
          <span id="model-sync-channel-label">{t('modelManagement.sync.channel.label')}</span>
          <Select
            aria-labelledby="model-sync-channel-label"
            items={channelItems}
            size="sm"
            isDisabled={props.loading || props.error}
            selectedKeys={[props.channelId === undefined ? PLACEHOLDER_CHANNEL_KEY : String(props.channelId)]}
            onSelectionChange={(keys) => {
              const next = String(Array.from(keys)[0] ?? PLACEHOLDER_CHANNEL_KEY)
              props.onChannelChange(next === PLACEHOLDER_CHANNEL_KEY ? undefined : Number(next))
            }}
          >
            {(item) => <SelectItem key={item.key} isDisabled={item.disabled}>{item.label}</SelectItem>}
          </Select>
        </div>
        <Button type="button" color="primary" size="sm" isDisabled={!selected || selected.status !== 'enabled' || props.previewing} onClick={props.onPreview}>
          {props.previewing ? null : <ScanSearch className="size-3.5" aria-hidden="true" />}
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
          <Chip size="sm" variant="flat">{t(`channels.protocol.${selected.protocol}`)}</Chip>
          <Chip size="sm" variant="flat">{t(`channels.type.${selected.type}`)}</Chip>
          <Chip size="sm" variant="flat">{selected.timeout_secs === null ? t('channels.values.inheritTimeout') : t('channels.values.timeout', { value: selected.timeout_secs })}</Chip>
        </div>
      ) : null}

      {props.error ? (
        <div role="alert" className="flex flex-wrap items-center justify-between gap-2 text-xs text-destructive">
          <span>{t('modelManagement.sync.channel.error')}</span>
          <Button type="button" size="sm" variant="light" onClick={props.onRetry}>{t('modelManagement.actions.retry')}</Button>
        </div>
      ) : null}
      {props.moreError ? (
        <div role="alert" className="flex flex-wrap items-center justify-between gap-2 text-xs text-destructive">
          <span>{t('modelManagement.sync.channel.moreError')}</span>
          <Button type="button" size="sm" variant="light" onClick={props.onRetryMore}>{t('modelManagement.actions.retry')}</Button>
        </div>
      ) : null}
      {props.hasMore && !props.moreError ? (
        <Button type="button" size="sm" variant="light" className="justify-self-start" isDisabled={props.loadingMore} onClick={props.onLoadMore}>
          {t(props.loadingMore ? 'modelManagement.loadingMore' : 'modelManagement.sync.channel.loadMore')}
        </Button>
      ) : null}
    </div>
  )
}
