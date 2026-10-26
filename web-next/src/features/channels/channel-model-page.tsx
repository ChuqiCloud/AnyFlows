import { useEffect, useMemo, useState } from 'react'
import { Button, Checkbox, Chip, Skeleton } from '@heroui/react'
import { ArrowLeft, Check, KeyRound, LoaderCircle, RefreshCw, ScanSearch, Save } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ChannelModelEditor } from './channel-model-editor'
import { ChannelGroupSelector } from './channel-group-selector'
import { ChannelKeyValueEditor } from './channel-key-value-editor'
import { createKeyValueRow, defaultChannelValues, toUpdateRequest, type KeyValueRow } from './channel-form-model'
import { useAdminChannel, useUpdateAdminChannel } from './channel-api'
import { useCreateModelSyncPreview } from '@/features/model-management/model-sync-api'
import { supportsModelDiscovery } from './channel-capabilities'
import { navigateTo } from '@/lib/router-navigation'
import type { AdminModelSyncPreview, AdminModelSyncPreviewItem } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

type ChannelModelPageProps = {
  channelId?: number
}

/** 管理单个渠道实际接入的模型、映射和路由分组。全局模型资料不在此页编辑。 */
export function ChannelModelPage({ channelId }: ChannelModelPageProps) {
  const { t } = useTranslation()
  const channelQuery = useAdminChannel(channelId)
  const updateMutation = useUpdateAdminChannel()
  const discoverMutation = useCreateModelSyncPreview()
  const channel = channelQuery.data
  const [models, setModels] = useState<string[]>([])
  const [groups, setGroups] = useState<number[]>([])
  const [mapping, setMapping] = useState<KeyValueRow[]>([])
  const [preview, setPreview] = useState<AdminModelSyncPreview>()
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [error, setError] = useState<string>()
  const [saved, setSaved] = useState(false)

  useEffect(() => {
    if (!channel) return
    setModels([...channel.models])
    setGroups([...channel.group_ids])
    setMapping(Object.entries(channel.model_mapping).map(([key, value], index) => ({
      id: `mapping-${index}`,
      key,
      value,
    })))
  }, [channel])

  const mappingObject = useMemo(() => Object.fromEntries(
    mapping
      .map((row) => [row.key.trim(), row.value.trim()] as const)
      .filter(([key, value]) => key.length > 0 && value.length > 0),
  ), [mapping])
  const modelsInvalid = !isValidChannelModels(models)
  const mappingInvalid = !isValidChannelMapping(mapping)

  const saveRouting = async (
    nextModels = models,
    nextGroups = groups,
    nextMapping = mappingObject,
    nextMappingRows = mapping,
  ) => {
    if (!channel) return false
    setError(undefined)
    setSaved(false)
    if ((nextModels.length === 0) !== (nextGroups.length === 0)) {
      setError(t('channelModels.validation.routing'))
      return false
    }
    if (nextModels.length > 512 || nextGroups.length > 64 || nextModels.length * nextGroups.length > 4096) {
      setError(t('channelModels.validation.limit'))
      return false
    }
    if (!isValidChannelModels(nextModels) || !isValidChannelMapping(nextMappingRows)) {
      setError(t('channelModels.validation.configuration'))
      return false
    }
    const values = defaultChannelValues(channel)
    values.models = nextModels
    values.groupIds = nextGroups
    values.modelMapping = Object.entries(nextMapping).map(([key, value], index) => ({ id: `mapping-${index}`, key, value }))
    try {
      await updateMutation.mutateAsync({ id: channel.id, body: toUpdateRequest(values) })
      setSaved(true)
      return true
    } catch {
      setError(t('channelModels.errors.save'))
      return false
    }
  }

  const openCredentials = () => {
    if (channel) navigateTo(`/console/credentials?channel=${channel.id}`)
  }

  const discover = async () => {
    if (!channel) return
    setError(undefined)
    setSaved(false)
    try {
      const result = await discoverMutation.mutateAsync(channel.id)
      setPreview(result)
      setSelected(new Set(result.items
        .filter((item) => item.relation !== 'not_reported' && !models.includes(item.canonical_model))
        .map((item) => item.item_id)))
    } catch {
      setError(t('channelModels.errors.discovery'))
    }
  }

  const addDiscovered = async () => {
    if (!preview) return
    const items = preview.items.filter((item) => selected.has(item.item_id))
    if (items.length === 0) return
    const nextModels = [...new Set([...models, ...items.map((item) => item.canonical_model)])]
    const nextMapping = { ...mappingObject }
    for (const item of items) {
      if (item.upstream_model && item.upstream_model !== item.canonical_model) {
        nextMapping[item.canonical_model] = item.upstream_model
      }
    }
    setModels(nextModels)
    setMapping(Object.entries(nextMapping).map(([key, value], index) => ({ id: `mapping-${index}`, key, value })))
    setPreview(undefined)
    setSelected(new Set())
    if (groups.length === 0) {
      setError(undefined)
      setSaved(false)
      return
    }
    const nextMappingRows = Object.entries(nextMapping).map(([key, value], index) => ({ id: `mapping-${index}`, key, value }))
    await saveRouting(nextModels, groups, nextMapping, nextMappingRows)
  }

  if (!channelId) {
    return <div role="alert" className="rounded-xl border border-danger/25 bg-danger/8 p-4 text-sm text-danger">{t('channelModels.errors.load')}</div>
  }
  if (channelQuery.isPending) {
    return <div className="grid gap-3"><Skeleton className="h-20 rounded-xl" /><Skeleton className="h-56 rounded-xl" /></div>
  }
  if (channelQuery.isError || !channel) {
    return <div role="alert" className="rounded-xl border border-danger/25 bg-danger/8 p-4 text-sm text-danger">{t('channelModels.errors.load')}</div>
  }

  const discoverySupported = supportsModelDiscovery(channel.type, channel.protocol)
  const candidateItems = preview?.items.filter((item) => item.relation !== 'not_reported' && !models.includes(item.canonical_model)) ?? []

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div className="min-w-0">
          <Button type="button" size="sm" variant="light" className="mb-2 -ml-2" onPress={() => navigateTo('/console/channels')}>
            <ArrowLeft className="size-3.5" aria-hidden="true" />{t('channelModels.actions.back')}
          </Button>
          <h2 className="truncate text-lg font-semibold">{t('channelModels.title')}</h2>
          <p className="mt-1 truncate text-xs text-muted-foreground">{channel.name} · {t(`channels.protocol.${channel.protocol}`)} · #{channel.id}</p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" size="sm" variant="bordered" onPress={openCredentials}><KeyRound className="size-3.5" aria-hidden="true" />{t('channelModels.actions.credentials')}</Button>
          <Button type="button" size="sm" variant="bordered" isDisabled={channelQuery.isFetching} onPress={() => void channelQuery.refetch()}><RefreshCw className={cn('size-3.5', channelQuery.isFetching && 'animate-spin')} aria-hidden="true" />{t('channelModels.actions.refresh')}</Button>
        </div>
      </header>

      <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(18rem,0.42fr)]">
        <section className="grid gap-4 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4" aria-labelledby="channel-models-heading">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 id="channel-models-heading" className="text-sm font-semibold">{t('channelModels.sections.connected')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('channelModels.sections.connectedHint')}</p>
            </div>
            <Chip className="bg-info/10 text-info" size="sm" variant="flat">{t('channelModels.summary.models', { count: models.length })}</Chip>
          </div>
          <ChannelModelEditor id="channel-models" value={models} onChange={(value) => { setModels(value); setSaved(false) }} />
          <div className="grid gap-2 border-t border-[var(--hairline)] pt-4">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <h4 className="text-xs font-semibold">{t('channelModels.sections.mapping')}</h4>
                <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channelModels.sections.mappingHint')}</p>
              </div>
              <Chip size="sm" variant="flat">{t('channelModels.summary.mapping', { count: Object.keys(mappingObject).length })}</Chip>
            </div>
            <ChannelKeyValueEditor
              rows={mapping}
              keyLabel={t('channels.mapping.source')}
              valueLabel={t('channels.mapping.target')}
              addLabel={t('channels.mapping.add')}
              removeLabel={t('channels.mapping.remove')}
              emptyText={t('channelModels.mapping.empty')}
              createRow={() => createKeyValueRow('mapping')}
              onChange={(value) => { setMapping(value); setSaved(false) }}
            />
            {modelsInvalid || mappingInvalid ? <p role="alert" className="text-xs text-danger">{t('channelModels.validation.configuration')}</p> : null}
          </div>
        </section>

        <div className="grid content-start gap-4">
          <section className="grid gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4" aria-labelledby="channel-groups-heading">
            <div>
              <h3 id="channel-groups-heading" className="text-sm font-semibold">{t('channelModels.sections.groups')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('channelModels.sections.groupsHint')}</p>
            </div>
            <ChannelGroupSelector id="channel-groups" value={groups} onChange={(value) => { setGroups(value); setSaved(false) }} />
            <Chip className={cn(groups.length > 0 ? 'bg-success/10 text-success' : 'text-muted-foreground')} size="sm" variant="flat">{t('channelModels.summary.groups', { count: groups.length })}</Chip>
          </section>

          <section className="grid gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4" aria-labelledby="channel-discovery-heading">
            <div>
              <h3 id="channel-discovery-heading" className="text-sm font-semibold">{t('channelModels.sections.discovery')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('channelModels.sections.discoveryHint')}</p>
            </div>
            <Button type="button" color="primary" size="sm" isDisabled={!discoverySupported || discoverMutation.isPending} onPress={() => void discover()}>
              {discoverMutation.isPending ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <ScanSearch className="size-3.5" aria-hidden="true" />}
              {t(discoverMutation.isPending ? 'channelModels.actions.discovering' : 'channelModels.actions.discover')}
            </Button>
            {!discoverySupported ? <p className="text-[0.6875rem] text-muted-foreground">{t('channelModels.discovery.unsupported')}</p> : null}
            {preview ? (
              <div className="grid gap-2 border-t border-[var(--hairline)] pt-3">
                <div className="flex items-center justify-between gap-2"><span className="text-xs font-medium">{t('channelModels.discovery.candidates', { count: candidateItems.length })}</span><Button type="button" size="sm" variant="light" onPress={() => { setPreview(undefined); setSelected(new Set()) }}>{t('channelModels.actions.close')}</Button></div>
                {candidateItems.length > 0 ? candidateItems.map((item) => <DiscoveryItem key={item.item_id} item={item} selected={selected.has(item.item_id)} onChange={(checked) => setSelected((current) => { const next = new Set(current); if (checked) next.add(item.item_id); else next.delete(item.item_id); return next })} />) : <p className="text-xs text-muted-foreground">{t('channelModels.discovery.empty')}</p>}
                {candidateItems.length > 0 ? <Button type="button" size="sm" color="primary" isDisabled={selected.size === 0 || updateMutation.isPending} onPress={() => void addDiscovered()}><Check className="size-3.5" aria-hidden="true" />{t('channelModels.actions.addSelected', { count: selected.size })}</Button> : null}
              </div>
            ) : null}
          </section>
        </div>
      </div>

      {error ? <p role="alert" className="rounded-lg bg-danger/10 px-3 py-2 text-xs text-danger">{error}</p> : null}
      {saved ? <p role="status" className="rounded-lg bg-success/10 px-3 py-2 text-xs text-success">{t('channelModels.feedback.saved')}</p> : null}
      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] pt-3">
        <Button type="button" variant="bordered" onPress={() => navigateTo('/console/channels')}>{t('channelModels.actions.cancel')}</Button>
        <Button type="button" color="primary" isDisabled={updateMutation.isPending || modelsInvalid || mappingInvalid} onPress={() => void saveRouting()}>{updateMutation.isPending ? <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" /> : <Save className="size-3.5" aria-hidden="true" />}{t(updateMutation.isPending ? 'channelModels.actions.saving' : 'channelModels.actions.save')}</Button>
      </div>
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

function DiscoveryItem({ item, selected, onChange }: { item: AdminModelSyncPreviewItem; selected: boolean; onChange: (checked: boolean) => void }) {
  const { t } = useTranslation()
  return (
    <label className="flex cursor-pointer items-start gap-2 rounded-lg border border-[var(--hairline)] px-2.5 py-2 hover:bg-surface-2/50">
      <Checkbox size="sm" isSelected={selected} onValueChange={onChange} aria-label={item.canonical_model} />
      <span className="min-w-0 text-xs"><span className="block truncate font-mono font-medium" title={item.canonical_model}>{item.canonical_model}</span><span className="mt-1 block truncate text-[0.6875rem] text-muted-foreground">{item.upstream_model && item.upstream_model !== item.canonical_model ? `${t('channelModels.discovery.upstream')}: ${item.upstream_model}` : t(`modelManagement.sync.relations.${item.relation}.badge`)}</span></span>
    </label>
  )
}
