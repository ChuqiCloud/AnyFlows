import { Button, Chip } from '@heroui/react'
import { useEffect, useMemo, useState } from 'react'
import { CheckCircle2, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useAdminChannels } from '@/features/channels/channel-api'
import type { AdminModelSyncPreview, AdminModelSyncPreviewItem } from '@/lib/api/generated/types.gen'
import {
  modelSyncErrorCode,
  useApplyModelSyncPreview,
  useCreateModelSyncPreview,
} from './model-sync-api'
import { ModelSyncApplyDialog } from './model-sync-apply-dialog'
import {
  defaultModelSyncDraft,
  modelSyncDraftIsComplete,
  toModelSyncApplyItem,
  type ModelSyncDraft,
} from './model-sync-form-model'
import { ModelSyncItemSheet } from './model-sync-item-sheet'
import { ModelSyncPreviewList } from './model-sync-preview-list'
import { currentUnix, formatModelSyncExpiry, ModelSyncError } from './model-sync-status'
import { ModelSyncToolbar } from './model-sync-toolbar'

export function ModelSyncWorkspace() {
  const { t } = useTranslation()
  const channelsQuery = useAdminChannels()
  const previewMutation = useCreateModelSyncPreview()
  const applyMutation = useApplyModelSyncPreview()
  const [channelId, setChannelId] = useState<number>()
  const [preview, setPreview] = useState<AdminModelSyncPreview>()
  const [selected, setSelected] = useState<Set<number>>(() => new Set())
  const [drafts, setDrafts] = useState<Record<number, ModelSyncDraft>>({})
  const [editing, setEditing] = useState<AdminModelSyncPreviewItem>()
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [lastAppliedCount, setLastAppliedCount] = useState<number>()
  const [now, setNow] = useState(() => currentUnix())

  const channels = useMemo(() => channelsQuery.data?.pages.flatMap((page) => page.channels) ?? [], [channelsQuery.data])
  const complete = useMemo(() => new Set([...selected].filter((itemId) => modelSyncDraftIsComplete(drafts[itemId]))), [drafts, selected])
  const expired = preview !== undefined && now >= preview.expires_at
  const previewChannelName = preview
    ? channels.find((channel) => channel.id === preview.channel_id)?.name
    : undefined

  useEffect(() => {
    if (!preview) return
    const delay = Math.max(0, preview.expires_at * 1000 - Date.now() + 50)
    const timer = window.setTimeout(() => setNow(currentUnix()), delay)
    return () => window.clearTimeout(timer)
  }, [preview])

  const createPreview = async () => {
    if (!channelId) return
    try {
      const next = await previewMutation.mutateAsync(channelId)
      setPreview(next)
      setSelected(new Set())
      setDrafts({})
      setEditing(undefined)
      setLastAppliedCount(undefined)
      setNow(currentUnix())
    } catch {
      // 旧预览与本地填写保持不变，管理员可以依据稳定错误分类决定是否重试。
    }
  }

  const toggle = (item: AdminModelSyncPreviewItem, checked: boolean) => {
    setSelected((current) => {
      const next = new Set(current)
      if (checked) next.add(item.item_id)
      else next.delete(item.item_id)
      return next
    })
    if (checked && !drafts[item.item_id]) {
      setDrafts((current) => ({ ...current, [item.item_id]: defaultModelSyncDraft() }))
    }
  }

  const edit = (item: AdminModelSyncPreviewItem) => {
    if (!drafts[item.item_id]) {
      setDrafts((current) => ({ ...current, [item.item_id]: defaultModelSyncDraft() }))
    }
    setEditing(item)
  }

  const apply = async () => {
    if (!preview || expired || selected.size === 0 || complete.size !== selected.size) return
    const items = preview.items
      .filter((item) => selected.has(item.item_id))
      .map((item) => toModelSyncApplyItem(item, drafts[item.item_id]))
    try {
      const result = await applyMutation.mutateAsync({ previewId: preview.preview_id, body: { items } })
      setLastAppliedCount(result.models.length)
      setPreview(undefined)
      setSelected(new Set())
      setDrafts({})
      setEditing(undefined)
      setConfirmOpen(false)
    } catch {
      setConfirmOpen(false)
    }
  }

  const previewError = previewMutation.error ? modelSyncErrorCode(previewMutation.error) ?? 'unknown' : undefined
  const applyError = applyMutation.error ? modelSyncErrorCode(applyMutation.error) ?? 'unknown' : undefined

  return (
    <div className="grid gap-5">
      <ModelSyncToolbar
        channelId={channelId}
        channels={channels}
        error={channelsQuery.isError && channelsQuery.data === undefined}
        hasMore={Boolean(channelsQuery.hasNextPage)}
        loading={channelsQuery.isPending}
        loadingMore={channelsQuery.isFetchingNextPage}
        moreError={channelsQuery.isFetchNextPageError}
        previewing={previewMutation.isPending}
        onChannelChange={setChannelId}
        onLoadMore={() => void channelsQuery.fetchNextPage()}
        onPreview={() => void createPreview()}
        onRetry={() => void channelsQuery.refetch()}
        onRetryMore={() => void channelsQuery.fetchNextPage()}
      />

      {previewError ? <ModelSyncError code={previewError} /> : null}
      {lastAppliedCount !== undefined ? (
        <div role="status" className="flex items-start gap-2 rounded-xl border border-success/20 bg-success/8 p-3 text-xs text-success">
          <CheckCircle2 className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <span>{t('modelManagement.sync.apply.success', { count: lastAppliedCount })}</span>
        </div>
      ) : null}

      {preview ? (
        <section className="grid gap-3 border-t border-[var(--hairline)] pt-5" aria-labelledby="model-sync-preview-title">
          <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
            <div>
              <div className="flex flex-wrap items-center gap-2">
                <h3 id="model-sync-preview-title" className="text-sm font-semibold">{t('modelManagement.sync.preview.title')}</h3>
                <Chip size="sm" variant="flat">{previewChannelName ?? t('modelManagement.sync.preview.sourceUnknown')}</Chip>
                <Chip size="sm" variant="flat">{t(`channels.protocol.${preview.protocol}`)}</Chip>
                <Chip className={expired ? 'bg-destructive/10 text-destructive' : 'bg-success/10 text-success'} size="sm" variant="flat">
                  {t(expired ? 'modelManagement.sync.preview.expired' : 'modelManagement.sync.preview.expiresAt', { value: formatModelSyncExpiry(preview.expires_at) })}
                </Chip>
              </div>
              <p className="mt-1 text-xs text-muted-foreground">{t('modelManagement.sync.preview.summary', { count: preview.items.length, selected: selected.size })}</p>
            </div>
            <Button type="button" color="primary" size="sm" isDisabled={expired || selected.size === 0 || complete.size !== selected.size} onClick={() => setConfirmOpen(true)}>
              <ShieldCheck className="size-3.5" aria-hidden="true" />{t('modelManagement.sync.actions.reviewApply', { count: selected.size })}
            </Button>
          </header>
          {applyError ? <ModelSyncError code={applyError} /> : null}
          <ModelSyncPreviewList items={preview.items} selected={selected} complete={complete} onEdit={edit} onToggle={toggle} />
        </section>
      ) : null}

      <ModelSyncItemSheet
        draft={editing ? drafts[editing.item_id] : undefined}
        item={editing}
        onClose={() => setEditing(undefined)}
        onSave={(itemId, draft) => {
          setDrafts((current) => ({ ...current, [itemId]: draft }))
          setSelected((current) => new Set(current).add(itemId))
          setEditing(undefined)
        }}
      />
      <ModelSyncApplyDialog count={selected.size} open={confirmOpen} pending={applyMutation.isPending} onApply={() => void apply()} onOpenChange={setConfirmOpen} />
    </div>
  )
}
