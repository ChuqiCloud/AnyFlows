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

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
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

  useEffect(() => {
    if (newChannelId !== undefined && channels.some((channel) => channel.id === newChannelId)) return
    setNewChannelId(channels[0]?.id)
  }, [channels, newChannelId])

  useEffect(() => {
    if (newCredentialId !== undefined && selectedCredentials.some((credential) => credential.id === newCredentialId)) return
    setNewCredentialId(selectedCredentials[0]?.id)
  }, [newCredentialId, selectedCredentials])

  const availableChannels = channels
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
          <label className="grid gap-1.5 text-xs font-medium">
            {t('routes.form.channel')}
            <Select
              value={newChannelId === undefined ? '' : String(newChannelId)}
              disabled={loading || availableChannels.length === 0}
              onChange={(event) => setNewChannelId(parseId(event.target.value))}
            >
              <option value="">{t('routes.form.selectChannel')}</option>
              {availableChannels.map((channel) => (
                <option key={channel.id} value={channel.id}>
                  {channel.name} · #{channel.id}
                </option>
              ))}
            </Select>
          </label>
          <label className="grid gap-1.5 text-xs font-medium">
            {t('routes.form.credential')}
            <Select
              value={newCredentialId === undefined ? '' : String(newCredentialId)}
              disabled={loading || selectedCredentials.length === 0}
              onChange={(event) => setNewCredentialId(parseId(event.target.value))}
            >
              <option value="">{t('routes.form.selectCredential')}</option>
              {selectedCredentials.map((credential) => (
                <option key={credential.id} value={credential.id}>
                  {credentialLabel(credential, t)}
                </option>
              ))}
            </Select>
          </label>
          <Button type="button" size="sm" variant="secondary" disabled={!canAdd} onClick={addCandidate}>
            <Plus aria-hidden="true" />
            {t('routes.form.addCandidate')}
          </Button>
        </div>
        {loading ? <p className="text-[0.6875rem] text-muted-foreground">{t('routes.form.candidatesLoading')}</p> : null}
        {error ? (
          <div className="flex flex-wrap items-center gap-2">
            <p role="alert" className="text-[0.6875rem] text-destructive">{t('routes.form.candidatesLoadFailed')}</p>
            <Button type="button" size="sm" variant="ghost" onClick={() => { onRetryCatalog(); void selectedChannelQuery?.refetch() }}>{t('routes.form.retryCandidates')}</Button>
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
        isDragging && 'relative z-10 border-brand/50 bg-surface-2 shadow-lg',
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
        <label className="grid gap-1 text-[0.6875rem] text-muted-foreground">
          <span>{t('routes.form.candidatePosition', { position })}</span>
          <Select value={String(row.channelId)} onChange={(event) => updateChannel(parseId(event.target.value))}>
            {channel ? null : <option value={row.channelId}>{channelLabel}</option>}
            {channels.map((item) => <option key={item.id} value={item.id}>{item.name} · #{item.id}</option>)}
          </Select>
          {channel ? <Badge className="w-fit border-transparent bg-surface-2 text-muted-foreground">{t(`channels.status.${channel.status}`)}</Badge> : null}
        </label>
        <label className="grid gap-1 text-[0.6875rem] text-muted-foreground">
          <span>{t('routes.form.credential')}</span>
          <Select value={String(row.credentialId)} onChange={(event) => onChange({ credentialId: parseId(event.target.value) ?? 0, stats: undefined })}>
            {credential ? null : <option value={row.credentialId}>{t('routes.form.unknownCredential', { id: row.credentialId })}</option>}
            {credentials.map((item) => <option key={item.id} value={item.id}>{credentialLabel(item, t)}</option>)}
          </Select>
        </label>
      </div>

      <div className="flex items-center justify-between gap-2 border-t border-[var(--hairline)] pt-2 sm:border-t-0 sm:pt-0">
        <div className="flex flex-wrap items-center gap-1.5 text-[0.6875rem]">
          <Badge className="border-transparent bg-info/10 text-info">{t('routes.form.weight')}</Badge>
          <Input
            className="h-7 w-20 text-right font-mono text-xs"
            type="number"
            min={0}
            step={1}
            inputMode="numeric"
            value={row.weight}
            aria-label={t('routes.form.weight')}
            onChange={(event) => onChange({ weight: event.target.value })}
          />
          {credential ? <RuntimeBadge credential={credential} /> : null}
          <Switch checked={row.enabled} aria-label={t('routes.form.candidateEnabled')} onCheckedChange={(enabled) => onChange({ enabled })} />
        </div>
        <Button type="button" size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label={t('routes.form.removeCandidate')} title={t('routes.form.removeCandidate')} onClick={onRemove}>
          <Trash2 aria-hidden="true" />
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
      {stats.cooldownLevel > 0 ? <Badge className="border-transparent bg-warning/10 text-warning">{t('routes.form.cooldown', { level: stats.cooldownLevel })}</Badge> : null}
    </div>
  )
}

function RuntimeBadge({ credential }: { credential: AdminCredential }) {
  const { t } = useTranslation()
  const state = credentialRuntimeState(credential)
  const tone = state === 'available' ? 'bg-success/10 text-success' : state === 'cooling' ? 'bg-warning/10 text-warning' : 'bg-surface-2 text-muted-foreground'
  return <Badge className={cn('border-transparent', tone)}>{t(`credentials.runtime.${state}`)}</Badge>
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
