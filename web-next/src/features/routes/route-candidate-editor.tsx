import { useEffect, useMemo, useState } from 'react'
import {
  closestCenter,
  DndContext,
  KeyboardSensor,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core'
import {
  arrayMove,
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import { GripVertical, Plus, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button, Chip, Input, Select, SelectItem, Switch } from '@heroui/react'
import type { AdminChannel, AdminCredential } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { credentialRuntimeState } from '@/features/credentials/credential-model'
import { useAdminRouteCredentialCatalog } from './route-api'
import type { RouteCandidateRow, RouteFormErrorCode } from './route-form-model'
import { createRouteRowId } from './route-form-model'

type RouteCandidateEditorProps = {
  rows: RouteCandidateRow[]
  channels: readonly AdminChannel[]
  catalogLoading: boolean
  catalogError: boolean
  onRetryCatalog: () => void
  formError?: RouteFormErrorCode
  onChange: (rows: RouteCandidateRow[]) => void
}

/** 管理路由候选渠道、账号和权重；排序顺序会在提交时转为优先级。 */
export function RouteCandidateEditor({
  rows,
  channels,
  catalogLoading,
  catalogError,
  onRetryCatalog,
  formError,
  onChange,
}: RouteCandidateEditorProps) {
  const { t } = useTranslation()
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )
  const [newChannelId, setNewChannelId] = useState<number>()
  const [newCredentialId, setNewCredentialId] = useState<number>()
  const availableChannels = channels
  const requestedChannelIds = useMemo(() => {
    const ids = [...rows.map((row) => row.channelId)]
    if (newChannelId !== undefined) ids.push(newChannelId)
    else if (channels[0]) ids.push(channels[0].id)
    return [...new Set(ids)]
  }, [channels, newChannelId, rows])
  const credentialsQueries = useAdminRouteCredentialCatalog(requestedChannelIds, !catalogLoading && !catalogError)
  const credentialsByChannel = credentialsQueries.credentialsByChannel as Readonly<Record<number, readonly AdminCredential[]>>
  const selectedCredentials = useMemo(
    () => newChannelId === undefined ? [] : credentialsByChannel[newChannelId] ?? [],
    [credentialsByChannel, newChannelId],
  )
  // HeroUI Select 的动态选项必须走 items + 渲染函数，这里预生成 key/label。
  const channelItems = useMemo(
    () => availableChannels.map((channel) => ({ key: String(channel.id), label: `${channel.name} · #${channel.id}` })),
    [availableChannels],
  )
  const credentialItems = useMemo(
    () => selectedCredentials.map((credential) => ({ key: String(credential.id), label: credentialLabel(credential, t) })),
    [selectedCredentials, t],
  )

  useEffect(() => {
    if (newChannelId !== undefined && channels.some((channel) => channel.id === newChannelId)) return
    setNewChannelId(channels[0]?.id)
  }, [channels, newChannelId])

  useEffect(() => {
    if (newCredentialId !== undefined && selectedCredentials.some((credential) => credential.id === newCredentialId)) return
    setNewCredentialId(selectedCredentials[0]?.id)
  }, [newCredentialId, selectedCredentials])

  const selectedChannelQueryIndex = newChannelId === undefined
    ? -1
    : credentialsQueries.channelIds.indexOf(newChannelId)
  const selectedChannelQuery = selectedChannelQueryIndex >= 0 ? credentialsQueries.queries[selectedChannelQueryIndex] : undefined
  const loading = catalogLoading || selectedChannelQuery?.isPending === true
  const error = catalogError || selectedChannelQuery?.isError === true
  const canAdd = newChannelId !== undefined
    && newCredentialId !== undefined
    && !rows.some((row) => row.channelId === newChannelId && row.credentialId === newCredentialId)
    && rows.length < 64

  const addCandidate = () => {
    if (!canAdd || newChannelId === undefined || newCredentialId === undefined) return
    onChange([...rows, {
      id: createRouteRowId('candidate'),
      channelId: newChannelId,
      credentialId: newCredentialId,
      weight: '1',
      enabled: true,
    }])
  }

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event
    if (!over || active.id === over.id) return
    const oldIndex = rows.findIndex((row) => row.id === active.id)
    const newIndex = rows.findIndex((row) => row.id === over.id)
    if (oldIndex < 0 || newIndex < 0) return
    onChange(arrayMove(rows, oldIndex, newIndex))
  }

  return (
    <div className="grid gap-3">
      <div className="grid gap-2 rounded-lg border border-[var(--hairline)] bg-surface-2/30 p-3">
        <div className="grid gap-2 sm:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)_auto] sm:items-end">
          <div className="grid gap-1.5 text-xs font-medium">
            <span id="route-new-channel-label">{t('routes.form.channel')}</span>
            <Select
              aria-labelledby="route-new-channel-label"
              isDisabled={loading || availableChannels.length === 0}
              items={channelItems}
              placeholder={t('routes.form.selectChannel')}
              selectedKeys={newChannelId === undefined ? [] : [String(newChannelId)]}
              size="sm"
              onSelectionChange={(keys) => setNewChannelId(parseId(String(Array.from(keys)[0] ?? '')))}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          </div>
          <div className="grid gap-1.5 text-xs font-medium">
            <span id="route-new-credential-label">{t('routes.form.credential')}</span>
            <Select
              aria-labelledby="route-new-credential-label"
              isDisabled={loading || selectedCredentials.length === 0}
              items={credentialItems}
              placeholder={t('routes.form.selectCredential')}
              selectedKeys={newCredentialId === undefined ? [] : [String(newCredentialId)]}
              size="sm"
              onSelectionChange={(keys) => setNewCredentialId(parseId(String(Array.from(keys)[0] ?? '')))}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          </div>
          <Button type="button" size="sm" variant="bordered" isDisabled={!canAdd} onClick={addCandidate}>
            <Plus className="size-3.5" aria-hidden="true" />
            {t('routes.form.addCandidate')}
          </Button>
        </div>
        {loading ? <p className="text-[0.6875rem] text-muted-foreground">{t('routes.form.candidatesLoading')}</p> : null}
        {error ? (
          <div className="flex flex-wrap items-center gap-2">
            <p role="alert" className="text-[0.6875rem] text-destructive">{t('routes.form.candidatesLoadFailed')}</p>
            <Button type="button" size="sm" variant="light" onClick={() => { onRetryCatalog(); void selectedChannelQuery?.refetch() }}>{t('routes.form.retryCandidates')}</Button>
          </div>
        ) : null}
        {!loading && !error && availableChannels.length === 0 ? (
          <p className="text-[0.6875rem] text-muted-foreground">{t('routes.form.noCredentialCandidates')}</p>
        ) : null}
      </div>

      {formError ? <p className="text-xs text-destructive">{t(`routes.validation.${formError}`)}</p> : null}

      {rows.length === 0 ? (
        <div className="grid min-h-28 place-items-center rounded-lg border border-dashed border-[var(--hairline)] px-3 text-center text-xs text-muted-foreground">
          {t('routes.form.candidatesEmpty')}
        </div>
      ) : (
        <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
          <SortableContext items={rows.map((row) => row.id)} strategy={verticalListSortingStrategy}>
            <div className="grid gap-2" role="list" aria-label={t('routes.form.candidates')}>
              {rows.map((row, index) => (
                <SortableCandidateRow
                  key={row.id}
                  row={row}
                  position={index + 1}
                  channels={channels}
                  credentialsByChannel={credentialsByChannel}
                  onChange={(patch) => onChange(rows.map((item) => item.id === row.id ? { ...item, ...patch } : item))}
                  onRemove={() => onChange(rows.filter((item) => item.id !== row.id))}
                />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      )}
    </div>
  )
}

type SortableCandidateRowProps = {
  row: RouteCandidateRow
  position: number
  channels: readonly AdminChannel[]
  credentialsByChannel: Readonly<Record<number, readonly AdminCredential[]>>
  onChange: (patch: Partial<RouteCandidateRow>) => void
  onRemove: () => void
}

function SortableCandidateRow({ row, position, channels, credentialsByChannel, onChange, onRemove }: SortableCandidateRowProps) {
  const { t } = useTranslation()
  const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({ id: row.id })
  const style = { transform: CSS.Transform.toString(transform), transition }
  const channel = channels.find((item) => item.id === row.channelId)
  const credentials = credentialsByChannel[row.channelId] ?? []
  const credential = credentials.find((item) => item.id === row.credentialId)
  const channelLabel = channel ? `${channel.name} (#${channel.id})` : t('routes.form.unknownChannel', { id: row.channelId })
  // 当前值不在目录里时（渠道已删除）额外保留一条占位选项，避免选中态丢失。
  const rowChannelItems = [
    ...(channel ? [] : [{ key: String(row.channelId), label: channelLabel }]),
    ...channels.map((item) => ({ key: String(item.id), label: `${item.name} · #${item.id}` })),
  ]
  const rowCredentialItems = [
    ...(credential ? [] : [{ key: String(row.credentialId), label: t('routes.form.unknownCredential', { id: row.credentialId }) }]),
    ...credentials.map((item) => ({ key: String(item.id), label: credentialLabel(item, t) })),
  ]

  const updateChannel = (channelId: number | undefined) => {
    if (channelId === undefined) return
    const nextCredential = (credentialsByChannel[channelId] ?? [])[0]
    onChange({ channelId, credentialId: nextCredential?.id ?? 0, stats: undefined })
  }

  return (
    <article
      ref={setNodeRef}
      style={style}
      role="listitem"
      className={cn(
        'grid gap-2 rounded-lg border border-[var(--hairline)] bg-surface-1 p-2.5 sm:grid-cols-[auto_minmax(0,1fr)_auto] sm:items-center',
        isDragging && 'relative z-10 border-brand/50 bg-surface-2',
      )}
    >
      <button
        type="button"
        ref={setActivatorNodeRef}
        className="flex size-8 shrink-0 cursor-grab items-center justify-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground active:cursor-grabbing"
        aria-label={t('routes.form.reorderCandidate', { position })}
        title={t('routes.form.reorderCandidate', { position })}
        {...attributes}
        {...listeners}
      >
        <GripVertical className="size-4" aria-hidden="true" />
      </button>

      <div className="grid min-w-0 gap-2 sm:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]">
        <div className="grid gap-1 text-[0.6875rem] text-muted-foreground">
          <span id={`route-candidate-channel-${row.id}`}>{t('routes.form.candidatePosition', { position })}</span>
          <Select
            aria-labelledby={`route-candidate-channel-${row.id}`}
            items={rowChannelItems}
            selectedKeys={[String(row.channelId)]}
            size="sm"
            onSelectionChange={(keys) => updateChannel(parseId(String(Array.from(keys)[0] ?? '')))}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
          {channel ? <Chip className="w-fit bg-surface-2 text-muted-foreground" size="sm" variant="flat">{t(`channels.status.${channel.status}`)}</Chip> : null}
        </div>
        <div className="grid gap-1 text-[0.6875rem] text-muted-foreground">
          <span id={`route-candidate-credential-${row.id}`}>{t('routes.form.credential')}</span>
          <Select
            aria-labelledby={`route-candidate-credential-${row.id}`}
            items={rowCredentialItems}
            selectedKeys={[String(row.credentialId)]}
            size="sm"
            onSelectionChange={(keys) => onChange({ credentialId: parseId(String(Array.from(keys)[0] ?? '')) ?? 0, stats: undefined })}
          >
            {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
          </Select>
        </div>
      </div>

      <div className="flex items-center justify-between gap-2 border-t border-[var(--hairline)] pt-2 sm:border-t-0 sm:pt-0">
        <div className="flex flex-wrap items-center gap-1.5 text-[0.6875rem]">
          <Chip className="bg-info/10 text-info" size="sm" variant="flat">{t('routes.form.weight')}</Chip>
          <Input
            aria-label={t('routes.form.weight')}
            className="w-20"
            classNames={{ input: 'h-7 text-right font-mono text-xs' }}
            inputMode="numeric"
            min={0}
            size="sm"
            step={1}
            type="number"
            value={row.weight}
            onChange={(event) => onChange({ weight: event.target.value })}
          />
          {credential ? <RuntimeBadge credential={credential} /> : null}
          <Switch isSelected={row.enabled} aria-label={t('routes.form.candidateEnabled')} size="sm" onValueChange={(enabled) => onChange({ enabled })} />
        </div>
        <Button isIconOnly type="button" size="sm" variant="light" className="text-muted-foreground hover:text-destructive" aria-label={t('routes.form.removeCandidate')} title={t('routes.form.removeCandidate')} onClick={onRemove}>
          <Trash2 className="size-3.5" aria-hidden="true" />
        </Button>
      </div>
      {row.stats ? <CandidateStats stats={row.stats} /> : null}
    </article>
  )
}

function CandidateStats({ stats }: { stats: NonNullable<RouteCandidateRow['stats']> }) {
  const { t } = useTranslation()
  const total = stats.successCount + stats.failCount
  const averageLatency = total > 0 ? Math.round(stats.totalLatencyMs / total) : undefined
  return (
    <div className="col-span-full flex flex-wrap gap-x-3 gap-y-1 border-t border-[var(--hairline)] pt-2 text-[0.625rem] text-muted-foreground tabular-nums sm:pl-10">
      <span>{t('routes.form.stats', { success: stats.successCount, failed: stats.failCount })}</span>
      <span>{averageLatency === undefined ? t('routes.form.statsNoLatency') : t('routes.form.statsLatency', { value: averageLatency })}</span>
      {stats.cooldownLevel > 0 ? <Chip className="bg-warning/10 text-warning" size="sm" variant="flat">{t('routes.form.cooldown', { level: stats.cooldownLevel })}</Chip> : null}
    </div>
  )
}

function RuntimeBadge({ credential }: { credential: AdminCredential }) {
  const { t } = useTranslation()
  const state = credentialRuntimeState(credential)
  const tone = state === 'available' ? 'bg-success/10 text-success' : state === 'cooling' ? 'bg-warning/10 text-warning' : 'bg-surface-2 text-muted-foreground'
  return <Chip className={cn(tone)} size="sm" variant="flat">{t(`credentials.runtime.${state}`)}</Chip>
}

function credentialLabel(credential: AdminCredential, t: (key: string, options?: Record<string, unknown>) => string) {
  const account = credential.oauth_account_key ? ` · ${redactAccountKey(credential.oauth_account_key)}` : ''
  return `${t(`credentials.kind.${credential.kind}`, { defaultValue: credential.kind })} #${credential.id}${account}`
}

/** 账号标识只用于区分候选，列表中不展示完整值，避免把上游账号信息扩散到页面。 */
function redactAccountKey(value: string) {
  if (value.length <= 4) return '***'
  return `${value.slice(0, 2)}***${value.slice(-2)}`
}

function parseId(value: string) {
  if (!/^[1-9]\d*$/.test(value)) return undefined
  const id = Number(value)
  return Number.isSafeInteger(id) ? id : undefined
}
