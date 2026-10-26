import { useEffect, useMemo, useState } from 'react'
import { ArrowLeft, Check, KeyRound, RefreshCw, Save, ScanSearch } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminModelSyncPreview } from '@/lib/api/generated/types.gen'
import { supportsModelDiscovery } from './channel-capabilities'
import { ChannelGroupSelector } from './channel-group-selector'
import { ChannelKeyValueEditor } from './channel-key-value-editor'
import { ChannelModelEditor } from './channel-model-editor'
import {
  createKeyValueRow,
  defaultChannelValues,
  toUpdateRequest,
  type KeyValueRow,
} from './channel-form-model'
import {
  useAdminChannel,
  useUpdateAdminChannel,
} from './channel-api'
import { useCreateModelSyncPreview } from '@/features/model-management/model-sync-api'

type ChannelModelPageProps = {
  channelId?: number
}

/** 管理单个渠道实际接入的模型和路由范围，避免在渠道基础表单中提前猜测模型。 */
export function ChannelModelPage({ channelId }: ChannelModelPageProps) {
  const { t } = useTranslation()
  const channelQuery = useAdminChannel(channelId)
  const updateMutation = useUpdateAdminChannel()
  const previewMutation = useCreateModelSyncPreview()
  const [models, setModels] = useState<string[]>([])
  const [groupIds, setGroupIds] = useState<number[]>([])
  const [mapping, setMapping] = useState<KeyValueRow[]>([])
  const [preview, setPreview] = useState<AdminModelSyncPreview>()
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [saved, setSaved] = useState(false)
  const [saveError, setSaveError] = useState(false)

  const channel = channelQuery.data
  const discoverySupported = channel ? supportsModelDiscovery(channel.type, channel.protocol) : false
  const modelsInvalid = !isValidChannelModels(models)
  const mappingInvalid = !isValidChannelMapping(mapping)
  const routingInvalid = (models.length === 0) !== (groupIds.length === 0)
    || models.length > 512
    || groupIds.length > 64
    || models.length * groupIds.length > 4096
  const selectedItems = useMemo(
    () => preview?.items.filter((item) => selected.has(item.item_id) && item.relation !== 'not_reported' && !models.includes(item.canonical_model)) ?? [],
    [models, preview, selected],
  )

  useEffect(() => {
    if (!channel) return
    setModels([...channel.models])
    setGroupIds([...channel.group_ids])
    setMapping(Object.entries(channel.model_mapping).map(([key, value], index) => ({
      id: `channel-model-mapping-${index}`,
      key,
      value,
    })))
    setPreview(undefined)
    setSelected(new Set())
    setSaved(false)
    setSaveError(false)
  }, [channel])

  const saveRouting = async () => {
    if (!channel || routingInvalid || modelsInvalid || mappingInvalid) return false
    const values = defaultChannelValues(channel)
    values.models = models
    values.groupIds = groupIds
    values.modelMapping = mapping
      .filter((row) => row.key.trim().length > 0 && row.value.trim().length > 0)
      .map((row) => ({ ...row, key: row.key.trim(), value: row.value.trim() }))
    setSaved(false)
    setSaveError(false)
    try {
      await updateMutation.mutateAsync({ id: channel.id, body: toUpdateRequest(values) })
      setSaved(true)
      return true
    } catch {
      setSaveError(true)
      return false
    }
  }

  const discover = async () => {
    if (!channel || !discoverySupported) return
    try {
      const result = await previewMutation.mutateAsync(channel.id)
      setPreview(result)
      setSelected(new Set(result.items
        .filter((item) => item.relation !== 'not_reported' && !models.includes(item.canonical_model))
        .map((item) => item.item_id)))
    } catch {
      // Mutation state renders the localized discovery error below.
    }
  }

  const addDiscovered = async () => {
    if (!preview || selectedItems.length === 0 || !channel) return
    const nextModels = [...new Set([...models, ...selectedItems.map((item) => item.canonical_model)])]
    const nextMapping = new Map(
      mapping
        .filter((row) => row.key.trim().length > 0 && row.value.trim().length > 0)
        .map((row) => [row.key.trim(), row.value.trim()]),
    )
    for (const item of selectedItems) {
      if (item.upstream_model && item.upstream_model !== item.canonical_model) {
        nextMapping.set(item.canonical_model, item.upstream_model)
      }
    }
    setModels(nextModels)
    setMapping([...nextMapping].map(([key, value], index) => ({ id: `channel-model-mapping-${index}`, key, value })))
    setPreview(undefined)
    setSelected(new Set())
    setSaved(false)
    if (groupIds.length === 0) {
      setSaveError(false)
      return
    }
    const values = defaultChannelValues(channel)
    values.models = nextModels
    values.groupIds = groupIds
    values.modelMapping = [...nextMapping].map(([key, value], index) => ({ id: `channel-model-mapping-${index}`, key, value }))
    try {
      await updateMutation.mutateAsync({ id: channel.id, body: toUpdateRequest(values) })
      setSaved(true)
    } catch {
      setSaveError(true)
    }
  }

  if (channelId === undefined) {
    return <ModelPageMessage title={t('channels.models.missingTitle')} body={t('channels.models.missingBody')} />
  }
  if (channelQuery.isPending) {
    return <div className="grid gap-3"><Skeleton className="h-24 rounded-xl" /><Skeleton className="h-64 rounded-xl" /></div>
  }
  if (channelQuery.isError || !channel) {
    return <ModelPageMessage title={t('channels.models.errorTitle')} body={t('channels.models.errorBody')} onRetry={() => void channelQuery.refetch()} />
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <Button type="button" size="xs" variant="ghost" className="mb-2 -ml-2" onClick={() => { window.location.hash = '#/console/channels' }}>
            <ArrowLeft aria-hidden="true" />{t('channels.models.back')}
          </Button>
          <h2 className="truncate text-lg font-semibold">{t('channels.models.title')}</h2>
          <p className="mt-1 text-xs text-muted-foreground">
            <span className="font-medium text-foreground">{channel.name}</span>
            <span className="mx-1.5">·</span>
            {t(`channels.protocol.${channel.protocol}`)}
            <span className="mx-1.5">·</span>
            {t('channels.models.subtitle')}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button type="button" size="sm" variant="secondary" onClick={() => { window.location.hash = `#/console/credentials?channel=${channel.id}` }}>
            <KeyRound aria-hidden="true" />{t('channels.models.credentials')}
          </Button>
          <Button type="button" size="sm" disabled={updateMutation.isPending || routingInvalid || modelsInvalid || mappingInvalid} onClick={() => void saveRouting()}>
            {updateMutation.isPending ? <RefreshCw className="animate-spin" aria-hidden="true" /> : <Save aria-hidden="true" />}
            {t(updateMutation.isPending ? 'channels.models.saving' : 'channels.models.save')}
          </Button>
        </div>
      </header>

      <section className="grid gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h3 className="text-sm font-semibold">{t('channels.models.connectedTitle')}</h3>
            <p className="mt-1 text-xs text-muted-foreground">{t('channels.models.connectedHint')}</p>
          </div>
          <div className="flex items-center gap-2">
            <Badge>{t('channels.models.modelCount', { count: models.length })}</Badge>
            <Badge>{t('channels.models.groupCount', { count: groupIds.length })}</Badge>
          </div>
        </div>
        <ChannelModelEditor id="channel-models-page" value={models} onChange={(value) => { setSaved(false); setModels(value) }} />
        <div className="grid gap-2 border-t border-[var(--hairline)] pt-3">
          <p className="text-xs font-medium">{t('channels.fields.groupIds')}</p>
          <ChannelGroupSelector id="channel-groups-page" value={groupIds} invalid={routingInvalid} onChange={(value) => { setSaved(false); setGroupIds(value) }} />
          {routingInvalid ? <p role="alert" className="text-xs text-destructive">{t('channels.models.routingInvalid')}</p> : null}
          {!routingInvalid && (modelsInvalid || mappingInvalid) ? <p role="alert" className="text-xs text-destructive">{t('channels.models.configurationInvalid')}</p> : null}
          {saved ? <p className="inline-flex items-center gap-1 text-xs text-success"><Check className="size-3.5" aria-hidden="true" />{t('channels.models.saved')}</p> : null}
        </div>
      </section>

      <section className="grid gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <h3 className="text-sm font-semibold">{t('channels.models.mappingTitle')}</h3>
            <p className="mt-1 text-xs text-muted-foreground">{t('channels.models.mappingHint')}</p>
          </div>
          <Badge>{t('channels.models.mappingCount', { count: mapping.length })}</Badge>
        </div>
        <ChannelKeyValueEditor
          rows={mapping}
          keyLabel={t('channels.mapping.source')}
          valueLabel={t('channels.mapping.target')}
          addLabel={t('channels.mapping.add')}
          removeLabel={t('channels.mapping.remove')}
          emptyText={t('channels.mapping.empty')}
          createRow={() => createKeyValueRow('channel-model-mapping')}
          onChange={(value) => { setSaved(false); setMapping(value) }}
        />
      </section>

      <section className="grid gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <h3 className="text-sm font-semibold">{t('channels.models.discoveryTitle')}</h3>
            <p className="mt-1 text-xs text-muted-foreground">{t(discoverySupported ? 'channels.models.discoveryHint' : 'channels.models.discoveryUnsupported')}</p>
          </div>
          <Button type="button" size="sm" variant="secondary" disabled={!discoverySupported || previewMutation.isPending} onClick={() => void discover()}>
            {previewMutation.isPending ? <RefreshCw className="animate-spin" aria-hidden="true" /> : <ScanSearch aria-hidden="true" />}
            {t(previewMutation.isPending ? 'channels.models.discovering' : 'channels.models.discover')}
          </Button>
        </div>
        {preview ? <DiscoveryPreview preview={preview} models={models} selected={selected} onSelectedChange={setSelected} onApply={() => void addDiscovered()} applying={updateMutation.isPending} selectedCount={selectedItems.length} /> : null}
        {previewMutation.isError ? <p role="alert" className="text-xs text-destructive">{t('channels.models.discoveryError')}</p> : null}
      </section>
      {saveError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('channels.models.saveError')}</p> : null}
    </div>
  )
}

function isValidChannelModels(models: string[]) {
  return models.length <= 512
    && new Set(models).size === models.length
    && models.every((model) => model.length > 0 && model.trim() === model && new TextEncoder().encode(model).length <= 255)
}

function isValidChannelMapping(rows: KeyValueRow[]) {
  const entries = rows.map((row) => [row.key.trim(), row.value.trim()] as const)
  if (entries.length > 512 || entries.some(([key, value]) => key.length === 0 || value.length === 0 || new TextEncoder().encode(key).length > 256 || new TextEncoder().encode(value).length > 256)) return false
  if (new Set(entries.map(([key]) => key)).size !== entries.length) return false
  return new TextEncoder().encode(JSON.stringify(Object.fromEntries(entries))).length <= 64 * 1024
}

function DiscoveryPreview({ preview, models, selected, onSelectedChange, onApply, applying, selectedCount }: {
  preview: AdminModelSyncPreview
  models: string[]
  selected: ReadonlySet<number>
  onSelectedChange: (value: Set<number>) => void
  onApply: () => void
  applying: boolean
  selectedCount: number
}) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-3 border-t border-[var(--hairline)] pt-3">
      <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
        <span>{t('channels.models.discoveryResult', { count: preview.items.length })}</span>
        <Button type="button" size="xs" disabled={applying || selectedCount === 0} onClick={onApply}>
          {applying ? <RefreshCw className="animate-spin" aria-hidden="true" /> : null}
          {t('channels.models.addSelected', { count: selectedCount })}
        </Button>
      </div>
      <div className="divide-y divide-[var(--hairline)] rounded-lg border border-[var(--hairline)]">
        {preview.items.map((item) => {
          const disabled = item.relation === 'not_reported' || models.includes(item.canonical_model)
          return (
            <label key={item.item_id} className="flex items-start gap-3 px-3 py-2.5 text-xs">
              <Checkbox checked={selected.has(item.item_id)} disabled={disabled} onCheckedChange={(checked) => {
                const next = new Set(selected)
                if (checked === true) next.add(item.item_id)
                else next.delete(item.item_id)
                onSelectedChange(next)
              }} />
              <span className="min-w-0 flex-1">
                <span className="block truncate font-mono font-medium">{item.canonical_model}</span>
                <span className="mt-1 block truncate text-muted-foreground">{item.upstream_model ?? t('channels.models.notReported')}</span>
              </span>
              <Badge className={item.relation === 'missing_metadata' ? 'border-transparent bg-warning/10 text-warning' : item.relation === 'existing' ? 'border-transparent bg-success/10 text-success' : 'border-transparent bg-info/10 text-info'}>
                {t(`channels.models.relations.${item.relation}`)}
              </Badge>
            </label>
          )
        })}
      </div>
    </div>
  )
}

function ModelPageMessage({ title, body, onRetry }: { title: string; body: string; onRetry?: () => void }) {
  const { t } = useTranslation()
  return (
    <div className="grid min-h-64 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1 p-6 text-center">
      <div>
        <h2 className="text-sm font-semibold">{title}</h2>
        <p className="mt-1 text-xs text-muted-foreground">{body}</p>
        {onRetry ? <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={onRetry}>{t('channels.actions.retry')}</Button> : null}
      </div>
    </div>
  )
}
