import { useEffect, useState, type ReactNode } from 'react'
import { Button, Checkbox, Input, Skeleton } from '@heroui/react'
import { ArrowLeft, ChevronDown, ScanSearch } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useCreateModelSyncPreview } from '@/features/model-management/model-sync-api'
import { UpstreamModelPolicyEditor } from '@/features/credentials/upstream-model-policy-editor'
import { ChannelModelCoverage } from '@/features/credentials/channel-model-coverage'
import type { AdminChannel, AdminModelSyncPreviewItem } from '@/lib/api/generated/types.gen'
import { navigateTo } from '@/lib/router-navigation'
import { ChannelModelEditor } from './channel-model-editor'
import { ChannelGroupSelector } from './channel-group-selector'
import { ChannelKeyValueEditor } from './channel-key-value-editor'
import { createKeyValueRow, defaultChannelValues, toUpdateRequest, type KeyValueRow } from './channel-form-model'
import { useAdminChannel, useUpdateAdminChannel } from './channel-api'
import { supportsModelDiscovery } from './channel-capabilities'

type RoutingDraft = {
  models: string[]
  groups: number[]
  mapping: KeyValueRow[]
}

function routingFromChannel(channel: AdminChannel): RoutingDraft {
  return {
    models: channel.models,
    groups: channel.group_ids,
    mapping: Object.entries(channel.model_mapping).map(([key, value], index) => ({
      id: `mapping-${index}`, key, value,
    })),
  }
}

export function ChannelModelPage({ channelId }: { channelId?: number }) {
  const { t } = useTranslation()
  const query = useAdminChannel(channelId)

  if (channelId && query.isPending) {
    return <div aria-busy="true" className="grid gap-3"><Skeleton className="h-16 rounded-xl" /><Skeleton className="h-56 rounded-xl" /></div>
  }
  if (!channelId || !query.data) {
    return <div role="alert" className="flex flex-wrap items-center gap-3 rounded-xl border border-danger/25 bg-danger/8 p-4 text-sm text-danger">
      {t('channelModels.errors.load')}
      {channelId ? <Button size="sm" variant="light" isLoading={query.isFetching} onPress={() => void query.refetch()}>{t('channelModels.actions.retry')}</Button> : null}
    </div>
  }

  return <ChannelModelWorkspace key={channelId} channel={query.data} refresh={query.refetch} refreshing={query.isFetching} />
}

function ChannelModelWorkspace({ channel, refresh, refreshing }: {
  channel: AdminChannel
  refresh: () => Promise<unknown>
  refreshing: boolean
}) {
  const { t } = useTranslation()
  const update = useUpdateAdminChannel()
  const discovery = useCreateModelSyncPreview()
  const [draft, setDraft] = useState<RoutingDraft>()
  const [modelInput, setModelInput] = useState('')
  const [preview, setPreview] = useState<AdminModelSyncPreviewItem[]>()
  const [selected, setSelected] = useState<Set<number>>(new Set())
  const [search, setSearch] = useState('')
  const [advancedOpened, setAdvancedOpened] = useState(false)
  const [saved, setSaved] = useState(false)
  const [addedCount, setAddedCount] = useState(0)
  const [saveError, setSaveError] = useState(false)
  const routing = draft ?? routingFromChannel(channel)
  const { models, groups, mapping } = routing
  const dirty = draft !== undefined || Boolean(modelInput.trim())
  const busy = update.isPending
  const modelsInvalid = !isValidChannelModels(models)
  const mappingInvalid = !isValidChannelMapping(mapping)
  const limitExceeded = models.length > 512 || groups.length > 64 || models.length * groups.length > 4096
  const routingError = limitExceeded ? 'limit'
    : models.length > 0 && groups.length === 0 ? 'pickGroup'
    : groups.length > 0 && models.length === 0 ? 'addModel'
    : undefined
  const discoverySupported = supportsModelDiscovery(channel.type, channel.protocol)
  const candidates = preview?.filter((item) => !models.includes(item.canonical_model)) ?? []
  const selectedCount = candidates.filter((item) => selected.has(item.item_id)).length
  const searchTerm = search.trim().toLowerCase()
  const visible = candidates.filter((item) =>
    item.canonical_model.toLowerCase().includes(searchTerm) || item.upstream_model?.toLowerCase().includes(searchTerm),
  )
  const visibleSelected = visible.filter((item) => selected.has(item.item_id)).length

  useEffect(() => {
    if (!dirty && !busy) return
    const preventUnload = (event: BeforeUnloadEvent) => { event.preventDefault(); event.returnValue = '' }
    window.addEventListener('beforeunload', preventUnload)
    return () => window.removeEventListener('beforeunload', preventUnload)
  }, [dirty, busy])

  const edit = (change: Partial<RoutingDraft>) => {
    if (busy) return
    setDraft({ ...routing, ...change })
    setSaved(false)
    setSaveError(false)
    setAddedCount(0)
  }

  const leave = (path: string) => {
    if (!busy && (!dirty || window.confirm(t('channelModels.feedback.discard')))) navigateTo(path)
  }

  const resetPreview = () => {
    setPreview(undefined)
    setSelected(new Set())
    setSearch('')
    discovery.reset()
  }

  const discover = async () => {
    try {
      const result = await discovery.mutateAsync(channel.id)
      const unique = new Map<number, AdminModelSyncPreviewItem>()
      const names = new Set(models)
      for (const item of result.items) {
        if (item.relation === 'not_reported' || names.has(item.canonical_model) || unique.has(item.item_id)) continue
        unique.set(item.item_id, item)
        names.add(item.canonical_model)
      }
      setPreview([...unique.values()])
      setSelected(new Set(unique.keys()))
      setSearch('')
    } catch {
      // The mutation displays its error beside the discovery action.
    }
  }

  const addDiscovered = () => {
    const items = candidates.filter((item) => selected.has(item.item_id))
    if (!items.length) return
    const existingMappings = new Set(mapping.map(({ key }) => key.trim()))
    const nextMapping = [...mapping]
    for (const item of items) {
      const { canonical_model: model, upstream_model: upstream } = item
      if (upstream && upstream !== model && !existingMappings.has(model)) {
        nextMapping.push({ ...createKeyValueRow('mapping'), key: model, value: upstream })
      }
    }
    edit({ models: [...models, ...items.map((item) => item.canonical_model)], mapping: nextMapping })
    setAddedCount(items.length)
    resetPreview()
  }

  const save = async () => {
    if (!draft || busy || modelsInvalid || mappingInvalid || routingError || modelInput.trim()) return
    setSaveError(false)
    const values = defaultChannelValues(channel)
    values.models = models
    values.groupIds = groups
    values.modelMapping = mapping.map((row) => ({ ...row, key: row.key.trim(), value: row.value.trim() }))
    try {
      await update.mutateAsync({ id: channel.id, body: toUpdateRequest(values) })
      setDraft(undefined)
      setAddedCount(0)
      setSaved(true)
    } catch {
      setSaveError(true)
    }
  }

  return <div className="mx-auto flex w-full max-w-5xl min-w-0 flex-col gap-5">
    <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
      <div className="min-w-0">
        <Button type="button" size="sm" variant="light" className="-ms-2 mb-2" isDisabled={busy} onPress={() => leave('/console/channels')}>
          <ArrowLeft className="size-4" aria-hidden="true" />{t('channelModels.actions.back')}
        </Button>
        <h2 className="text-xl font-semibold">{t('channelModels.title')}</h2>
        <p className="mt-1 break-words text-sm text-muted-foreground">{channel.name} · {t(`channels.protocol.${channel.protocol}`)}</p>
      </div>
      <Button type="button" size="sm" variant="bordered" className="self-start sm:self-end" isDisabled={busy}
        onPress={() => leave(`/console/credentials?channel=${channel.id}`)}>{t('channelModels.actions.credentials')}</Button>
    </header>

    <div className="rounded-xl border border-[var(--hairline)] bg-surface-1">
      <fieldset disabled={busy} className="min-w-0">
        <legend className="sr-only">{t('channelModels.title')}</legend>
        <section className="grid gap-3 p-4 sm:p-5" aria-labelledby="channel-models-heading">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h3 id="channel-models-heading" className="text-sm font-semibold">{t('channelModels.sections.connected')} <span className="ms-1 font-normal tabular-nums text-muted-foreground">{models.length}</span></h3>
            {discoverySupported ? <Button type="button" size="sm" variant="bordered" isLoading={discovery.isPending}
              isDisabled={busy || discovery.isPending} onPress={() => void discover()}>
              {!discovery.isPending ? <ScanSearch className="size-4" aria-hidden="true" /> : null}
              {t(discovery.isPending ? 'channelModels.actions.discovering' : 'channelModels.actions.discover')}
            </Button> : null}
          </div>

          <ChannelModelEditor id="channel-models" value={models} draft={modelInput} onDraftChange={setModelInput}
            invalid={modelsInvalid} disabled={busy} onChange={(value) => edit({ models: value })} />
          {modelsInvalid ? <p role="alert" className="text-sm text-danger">{t('channelModels.validation.models')}</p> : null}
          {discovery.isError ? <p role="alert" className="text-sm text-danger">{t('channelModels.errors.discovery')}</p> : null}
          {addedCount > 0 ? <p role="status" className="text-sm text-muted-foreground">{t('channelModels.feedback.added', { count: addedCount })}</p> : null}

          {preview ? <section className="grid min-w-0 gap-3 rounded-lg border border-[var(--hairline)] p-3 sm:p-4" aria-labelledby="channel-discovery-heading">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h4 id="channel-discovery-heading" className="text-sm font-semibold">{t('channelModels.discovery.candidates', { count: candidates.length })}</h4>
              <Button type="button" size="sm" variant="light" isDisabled={busy || discovery.isPending} onPress={resetPreview}>{t('channelModels.actions.close')}</Button>
            </div>
            {candidates.length > 0 ? <>
              {candidates.length > 8 ? <Input size="sm" aria-label={t('channelModels.discovery.search')}
                placeholder={t('channelModels.discovery.search')} value={search} onValueChange={setSearch} /> : null}
              {visible.length > 0 ? <>
                <Checkbox size="sm" isSelected={visibleSelected === visible.length} isIndeterminate={visibleSelected > 0 && visibleSelected < visible.length}
                  onValueChange={(checked) => setSelected((current) => {
                    const next = new Set(current)
                    for (const item of visible) {
                      if (checked) next.add(item.item_id)
                      else next.delete(item.item_id)
                    }
                    return next
                  })}>{t('channelModels.discovery.selectVisible')}</Checkbox>
                <div className="max-h-64 divide-y divide-[var(--hairline)] overflow-auto rounded-md border border-[var(--hairline)]">
                  {visible.map((item) => <Checkbox key={item.item_id} size="sm" classNames={{ base: 'm-0 w-full max-w-none px-3 py-2', label: 'min-w-0' }}
                    isSelected={selected.has(item.item_id)} onValueChange={(checked) => setSelected((current) => {
                      const next = new Set(current)
                      if (checked) next.add(item.item_id)
                      else next.delete(item.item_id)
                      return next
                    })}>
                    <span className="block break-all text-sm font-medium">{item.canonical_model}</span>
                    {item.upstream_model && item.upstream_model !== item.canonical_model
                      ? <span className="block break-all text-xs text-muted-foreground">{t('channelModels.discovery.upstream', { model: item.upstream_model })}</span> : null}
                  </Checkbox>)}
                </div>
              </> : <p className="text-sm text-muted-foreground">{t('channelModels.discovery.noMatch')}</p>}
              <Button type="button" size="sm" variant="bordered" className="justify-self-start"
                isDisabled={busy || discovery.isPending || selectedCount === 0 || models.length + selectedCount > 512}
                onPress={addDiscovered}>{t('channelModels.actions.addSelected', { count: selectedCount })}</Button>
              {models.length + selectedCount > 512 ? <p role="alert" className="text-sm text-danger">{t('channelModels.discovery.limit')}</p> : null}
            </> : <p className="text-sm text-muted-foreground">{t('channelModels.discovery.empty')}</p>}
          </section> : null}
        </section>

        <section className="grid gap-2 border-t border-[var(--hairline)] p-4 sm:p-5" aria-labelledby="channel-groups-heading">
          <h3 id="channel-groups-heading" className="text-sm font-semibold">{t('channelModels.sections.groups')}</h3>
          <p className="text-xs text-muted-foreground">{t('channelModels.sections.groupsHint')}</p>
          <div className="sm:max-w-lg"><ChannelGroupSelector id="channel-groups" value={groups}
            invalid={routingError === 'pickGroup'} onChange={(value) => edit({ groups: value })} /></div>
          {routingError ? <p role="alert" className="text-sm text-danger">{t(`channelModels.validation.${routingError}`)}</p> : null}
        </section>

        <Disclosure title={t('channelModels.sections.mapping')} detail={mapping.length ? t('channelModels.summary.mapping', { count: mapping.length }) : t('channelModels.mapping.optional')}>
          <p className="text-sm text-muted-foreground">{t('channelModels.sections.mappingHint')}</p>
          <ChannelKeyValueEditor rows={mapping}
            keyLabel={t('channelModels.mapping.publicName')} valueLabel={t('channelModels.mapping.upstreamName')}
            addLabel={t('channels.mapping.add')} removeLabel={t('channels.mapping.remove')} emptyText={t('channelModels.mapping.empty')}
            createRow={() => createKeyValueRow('mapping')} onChange={(value) => edit({ mapping: value })} />
        </Disclosure>
        {mappingInvalid ? <p role="alert" className="px-4 pb-4 text-sm text-danger sm:px-5">{t('channelModels.validation.mapping')}</p> : null}
      </fieldset>

      <div className="sticky bottom-0 z-10 flex flex-wrap items-center justify-between gap-3 rounded-b-xl border-t border-[var(--hairline)] bg-surface-1 p-4 sm:px-5">
        <div className="min-w-0 flex-1 text-sm">
          {saveError ? <p role="alert" className="text-danger">{t('channelModels.errors.save')}</p>
            : saved ? <p role="status" className="text-success">{t('channelModels.feedback.saved')}</p>
            : dirty ? <p className="text-muted-foreground">{t(modelInput.trim() ? 'channelModels.feedback.finishAdding' : 'channelModels.feedback.unsaved')}</p> : null}
        </div>
        <Button type="button" color="primary" isLoading={busy}
          isDisabled={!draft || busy || modelsInvalid || mappingInvalid || Boolean(routingError) || Boolean(modelInput.trim())}
          onPress={() => void save()}>{t(busy ? 'channelModels.actions.saving' : 'channelModels.actions.save')}</Button>
      </div>
    </div>

    <Disclosure title={t('channelModels.sections.advanced')} detail={t('channelModels.sections.advancedHint')}
      standalone onOpen={() => setAdvancedOpened(true)}>
      {advancedOpened ? <>
        <section className="grid gap-3">
          <h3 className="text-sm font-semibold">{t('channelModels.sections.upstreamPolicy')}</h3>
          <UpstreamModelPolicyEditor channelId={channel.id} />
        </section>
        <ChannelModelCoverage channelId={channel.id} models={channel.models} />
        <Button type="button" size="sm" variant="light" className="justify-self-start" isDisabled={dirty || busy} isLoading={refreshing}
          onPress={() => { setSaved(false); void refresh() }}>{t('channelModels.actions.refresh')}</Button>
      </> : null}
    </Disclosure>
  </div>
}

function Disclosure({ title, detail, children, onOpen, standalone = false }: {
  title: string
  detail: string
  children: ReactNode
  onOpen?: () => void
  standalone?: boolean
}) {
  return <details className={standalone ? 'group rounded-xl border border-[var(--hairline)] bg-surface-1' : 'group border-t border-[var(--hairline)]'}
    onToggle={(event) => { if (event.currentTarget.open) onOpen?.() }}>
    <summary className="flex min-h-12 cursor-pointer list-none flex-wrap items-center gap-x-2 gap-y-1 rounded-md px-4 py-3 text-sm hover:bg-surface-2/50 focus-visible:outline-2 focus-visible:outline-primary sm:px-5 [&::-webkit-details-marker]:hidden">
      <ChevronDown className="size-4 shrink-0 transition-transform group-open:rotate-180" aria-hidden="true" />
      <span className="font-semibold">{title}</span>
      <span className="text-xs text-muted-foreground">{detail}</span>
    </summary>
    <div className="grid gap-3 px-4 pb-4 pt-2 sm:px-5 sm:pb-5">{children}</div>
  </details>
}

function isValidChannelModels(models: string[]) {
  return new Set(models).size === models.length
    && models.every((model) => model.length > 0 && model.trim() === model && new TextEncoder().encode(model).length <= 255)
}

function isValidChannelMapping(rows: KeyValueRow[]) {
  const entries = rows.map((row) => [row.key.trim(), row.value.trim()] as const)
  const encoder = new TextEncoder()
  if (entries.length > 512 || entries.some(([key, value]) => !key || !value || encoder.encode(key).length > 256 || encoder.encode(value).length > 256)) return false
  if (new Set(entries.map(([key]) => key)).size !== entries.length) return false
  return encoder.encode(JSON.stringify(Object.fromEntries(entries))).length <= 64 * 1024
}
