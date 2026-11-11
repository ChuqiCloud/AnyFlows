import { useEffect, useMemo, useState } from 'react'
import { Button, Input, Switch } from '@heroui/react'
import { Bell, ChevronDown, History, RefreshCw, Search, ShieldAlert } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useAdminChannels } from './channel-api'
import { useAdminChannelCredentials } from '@/features/credentials/credential-api'
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { ApiError } from '@/lib/api'
import { type ActiveMonitorTarget, type ActiveMonitorTargetWrite, useActiveMonitorEvents, useActiveMonitorMutation, useActiveMonitorSettings, useActiveMonitorTargets } from './active-channel-monitor-api'
import { PoolError } from '@/features/merchant-pool/merchant-pool-ui'

const lowCostProbeProtocols = new Set<AdminChannel['protocol']>([
  'openai_chat', 'openai_responses', 'openai_embeddings', 'anthropic', 'gemini', 'jina_rerank', 'cohere_rerank',
])

function supportsLowCostProbe(channel: AdminChannel) {
  if (!lowCostProbeProtocols.has(channel.protocol)) return false
  switch (channel.type) {
    case 'openai': return ['openai_chat', 'openai_responses', 'openai_embeddings'].includes(channel.protocol)
    case 'opencode': return false
    case 'anthropic': return channel.protocol === 'anthropic'
    case 'nvidia': return channel.protocol === 'openai_chat'
    case 'gemini': return channel.protocol === 'gemini'
    case 'jina': return channel.protocol === 'jina_rerank'
    case 'cohere': return channel.protocol === 'cohere_rerank'
    default: return false
  }
}

function isForbidden(error: unknown) {
  return error instanceof ApiError && error.status === 403
}

function targetWrite(target: ActiveMonitorTarget): ActiveMonitorTargetWrite {
  return {
    channel_id: target.channel_id,
    credential_id: target.credential_id,
    model: target.model,
    enabled: target.enabled,
    interval_secs: target.interval_secs,
    daily_budget: target.daily_budget,
    timeout_secs: target.timeout_secs,
    alert_after_secs: target.alert_after_secs,
    muted_until_epoch_seconds: target.muted_until_epoch_seconds,
  }
}

function MonitorTargetRow({ target, onDelete, onToggle, onMute, loading, t }: {
  target: ActiveMonitorTarget
  onDelete: (id: number) => void; onToggle: (target: ActiveMonitorTarget) => void; onMute: (target: ActiveMonitorTarget, muted: boolean) => void; loading: boolean; t: (key: string, values?: Record<string, unknown>) => string
}) {
  const [eventsOpen, setEventsOpen] = useState(false)
  const events = useActiveMonitorEvents(target.id, eventsOpen)
  return <article className="grid gap-2 border-t border-[var(--hairline)] py-3 sm:grid-cols-[minmax(0,1fr)_auto]">
    <div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><strong className="truncate text-xs">{target.model}</strong><span className="text-[11px] text-muted-foreground">{t('channels.activeMonitor.targetSummary', { channel: target.channel_id, credential: target.credential_id })}</span><span className={`rounded px-1.5 py-0.5 text-[10px] ${target.enabled ? 'bg-success/10 text-success' : 'bg-default/10 text-muted-foreground'}`}>{t(target.enabled ? 'channels.activeMonitor.targetEnabledStatus' : 'channels.activeMonitor.targetDisabledStatus')}</span>{target.alert_active ? <span className="inline-flex items-center gap-1 rounded bg-danger/10 px-1.5 py-0.5 text-[10px] text-danger"><Bell className="size-3" aria-hidden="true" />{t('channels.activeMonitor.alerting')}</span> : null}{target.alert_active && target.is_muted && target.muted_until_epoch_seconds ? <span className="rounded bg-warning/10 px-1.5 py-0.5 text-[10px] text-warning">{t('channels.activeMonitor.mutedUntil', { value: new Date(target.muted_until_epoch_seconds * 1000).toLocaleString() })}</span> : null}</div>
      <p className="mt-1 text-[11px] text-muted-foreground">{t('channels.activeMonitor.budgetUsed', { used: target.daily_used, budget: target.daily_budget })} · {t('channels.activeMonitor.intervalValue', { seconds: target.interval_secs })} · {t('channels.activeMonitor.timeoutValue', { seconds: target.timeout_secs })}</p>
      <p className="mt-1 text-[11px] text-muted-foreground">{t('channels.activeMonitor.lastProbe', { value: target.last_attempt_epoch_seconds ? new Date(target.last_attempt_epoch_seconds * 1000).toLocaleString() : t('channels.activeMonitor.neverProbed') })} · {t('channels.activeMonitor.lastSuccess', { value: target.last_success_epoch_seconds ? new Date(target.last_success_epoch_seconds * 1000).toLocaleString() : t('channels.activeMonitor.noSuccessfulProbe') })}</p>
      <div className="mt-1"><Button size="sm" variant="light" aria-expanded={eventsOpen} onPress={() => setEventsOpen((open) => !open)}><History className="size-3.5" aria-hidden="true" /><span>{t(eventsOpen ? 'channels.activeMonitor.hideEvents' : 'channels.activeMonitor.showEvents')}</span><ChevronDown className={`size-3.5 transition-transform ${eventsOpen ? 'rotate-180' : ''}`} aria-hidden="true" /></Button></div>
      {eventsOpen && events.isPending ? <span role="status" className="mt-1 block text-[10px] text-muted-foreground">{t('channels.activeMonitor.loadingEvents')}</span> : null}
      {eventsOpen && events.data?.length ? <div className="mt-2 flex flex-wrap gap-2">{events.data.slice(0, 4).map((event) => <span key={event.id} className={`rounded px-1.5 py-0.5 text-[10px] ${event.kind === 'down' ? 'bg-danger/10 text-danger' : 'bg-success/10 text-success'}`}>{t(`channels.activeMonitor.events.${event.kind}`)} · {new Date(event.occurred_epoch_seconds * 1000).toLocaleString()}</span>)}</div> : null}
      {eventsOpen && events.data?.length === 0 ? <span className="mt-1 block text-[10px] text-muted-foreground">{t('channels.activeMonitor.noEvents')}</span> : null}
      {eventsOpen && events.isError ? <span className="mt-1 block text-[10px] text-muted-foreground">{t('channels.activeMonitor.eventsUnavailable')}</span> : null}
    </div>
    <div className="flex flex-wrap items-center justify-end gap-2"><Switch size="sm" isSelected={target.enabled} isDisabled={loading} aria-label={t('channels.activeMonitor.targetEnabled')} onValueChange={(enabled) => onToggle({ ...target, enabled })} />{target.alert_active ? <Button size="sm" variant="flat" isDisabled={loading} onPress={() => onMute({ ...target }, !target.is_muted)}>{target.is_muted ? t('channels.activeMonitor.unmute') : t('channels.activeMonitor.muteOneHour')}</Button> : null}<Button size="sm" variant="light" color="danger" isDisabled={loading} onPress={() => onDelete(target.id)}>{t('channels.activeMonitor.remove')}</Button></div>
  </article>
}

export function ActiveChannelMonitorPanel() {
  const { t } = useTranslation()
  const settings = useActiveMonitorSettings()
  const targets = useActiveMonitorTargets()
  const channelsQuery = useAdminChannels(100)
  const channels = useMemo(() => channelsQuery.data?.pages.flatMap((page) => page.channels) ?? [], [channelsQuery.data])
  const probeChannels = useMemo(() => channels.filter(supportsLowCostProbe), [channels])
  const [channelId, setChannelId] = useState('')
  const channel = channels.find((item) => String(item.id) === channelId)
  const credentialsQuery = useAdminChannelCredentials(channel?.id)
  const credentials = useMemo(() => credentialsQuery.data?.pages.flatMap((page) => page.credentials) ?? [], [credentialsQuery.data])
  const [credentialId, setCredentialId] = useState('')
  const credential = credentials.find((item) => String(item.id) === credentialId)
  const models = channel && supportsLowCostProbe(channel) ? channel.models : []
  const [model, setModel] = useState('')
  const [interval, setInterval] = useState('300')
  const [budget, setBudget] = useState('12')
  const [timeoutSecs, setTimeoutSecs] = useState('15')
  const [alertAfter, setAlertAfter] = useState('900')
  const [globalBudget, setGlobalBudget] = useState('288')
  const [targetSearch, setTargetSearch] = useState('')
  const mutation = useActiveMonitorMutation()
  const saving = mutation.settings.isPending || mutation.saveTarget.isPending || mutation.deleteTarget.isPending
  useEffect(() => { if (settings.data) setGlobalBudget(String(settings.data.global_daily_budget)) }, [settings.data?.global_daily_budget])
  const targetRows = useMemo(() => {
    const query = targetSearch.trim().toLocaleLowerCase()
    return (targets.data ?? []).filter((target) => !query || `${target.model} ${target.channel_id} ${target.credential_id}`.toLocaleLowerCase().includes(query))
  }, [targetSearch, targets.data])
  const validTarget = Boolean(channel && channel.status === 'enabled' && credential?.status === 'enabled' && credential.schedulable && model && models.includes(model) && Number(interval) >= 300 && Number(interval) <= 86400 && Number(budget) >= 1 && Number(budget) <= 288 && Number(timeoutSecs) >= 1 && Number(timeoutSecs) <= 15 && Number(alertAfter) >= 60 && Number(alertAfter) <= 3600)

  const save = (enabled: boolean, global_daily_budget = settings.data?.global_daily_budget ?? 288) => void mutation.settings.mutateAsync({ enabled, global_daily_budget }).catch(() => undefined)
  const writeTarget = (target: ActiveMonitorTargetWrite) => void mutation.saveTarget.mutateAsync(target).catch(() => undefined)

  return <details className="rounded-lg border border-[var(--hairline)] bg-surface-1">
    <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-4 py-3 text-sm font-semibold [&::-webkit-details-marker]:hidden"><span className="inline-flex items-center gap-2"><ShieldAlert className="size-4 text-primary" aria-hidden="true" />{t('channels.activeMonitor.title')}</span><span className="text-[11px] font-normal text-muted-foreground">{t('channels.activeMonitor.summary')}</span></summary>
    <div className="grid gap-4 border-t border-[var(--hairline)] p-4">
      <p className="text-xs leading-5 text-muted-foreground">{t('channels.activeMonitor.description')}</p>
      {settings.isError || targets.isError ? <PoolError message={isForbidden(settings.error) || isForbidden(targets.error) ? t('channels.activeMonitor.forbidden') : t('channels.activeMonitor.loadError')} onRetry={() => { void settings.refetch(); void targets.refetch() }} /> : null}
      {settings.data ? <div className="grid gap-3 rounded-md border border-warning/20 bg-warning/5 p-3 sm:grid-cols-[minmax(0,1fr)_12rem_auto_auto] sm:items-end">
        <Switch isSelected={settings.data.enabled} isDisabled={saving} onValueChange={(enabled) => save(enabled)}>{t('channels.activeMonitor.globalEnabled')}</Switch>
        <label className="grid gap-1 text-xs">{t('channels.activeMonitor.globalBudget')}<Input size="sm" type="number" min={0} max={288} value={globalBudget} isDisabled={saving} onValueChange={setGlobalBudget} /></label>
        <Button size="sm" color="primary" isDisabled={saving || Number(globalBudget) < 0 || Number(globalBudget) > 288 || Number(globalBudget) === settings.data.global_daily_budget} onPress={() => save(settings.data?.enabled ?? false, Number(globalBudget))}>{t('channels.activeMonitor.saveSettings')}</Button>
        <Button size="sm" variant="bordered" isDisabled={settings.isFetching} onPress={() => { void settings.refetch(); void targets.refetch() }}><RefreshCw className="size-3.5" aria-hidden="true" />{t('channels.actions.refresh')}</Button>
      </div> : null}
      {targets.data?.length ? <label className="relative block max-w-md"><Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" /><Input size="sm" aria-label={t('channels.activeMonitor.searchTargets')} placeholder={t('channels.activeMonitor.searchTargets')} value={targetSearch} onValueChange={setTargetSearch} className="pl-8" /></label> : null}
      {targetRows.map((target) => <MonitorTargetRow key={target.id} target={target} loading={saving} t={t} onDelete={(id) => { void mutation.deleteTarget.mutateAsync(id).catch(() => undefined) }} onToggle={(current) => writeTarget(targetWrite(current))} onMute={(current, muted) => writeTarget({ ...targetWrite(current), muted_until_epoch_seconds: muted ? Math.floor(Date.now() / 1000) + 3600 : null })} />)}
      {targets.data?.length === 0 ? <p className="text-xs text-muted-foreground">{t('channels.activeMonitor.empty')}</p> : null}
      {targets.data?.length && targetRows.length === 0 ? <p className="text-xs text-muted-foreground">{t('channels.activeMonitor.noMatchingTargets')}</p> : null}
      {channelsQuery.isError ? <p role="alert" className="text-xs text-danger">{t(isForbidden(channelsQuery.error) ? 'channels.activeMonitor.forbidden' : 'channels.activeMonitor.channelsUnavailable')}</p> : null}
      {channelsQuery.hasNextPage ? <Button size="sm" variant="light" isDisabled={channelsQuery.isFetchingNextPage} onPress={() => void channelsQuery.fetchNextPage()}>{t('channels.activeMonitor.moreChannels')}</Button> : null}
      <fieldset className="grid gap-3 border-t border-[var(--hairline)] pt-4" disabled={saving}>
        <legend className="px-1 text-xs font-semibold">{t('channels.activeMonitor.addTarget')}</legend>
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          <label className="grid gap-1 text-xs">{t('channels.activeMonitor.channel')}<select className="h-9 min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-3" value={channelId} onChange={(event) => { setChannelId(event.target.value); setCredentialId(''); setModel('') }}><option value="">{t('channels.activeMonitor.choose')}</option>{probeChannels.filter((item) => item.status === 'enabled').map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
          <label className="grid gap-1 text-xs">{t('channels.activeMonitor.credential')}<select className="h-9 min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-3" value={credentialId} disabled={!channel || credentialsQuery.isPending} onChange={(event) => setCredentialId(event.target.value)}><option value="">{t('channels.activeMonitor.choose')}</option>{credentials.filter((item) => item.status === 'enabled' && item.schedulable && !item.oauth_token_pending).map((item) => <option key={item.id} value={item.id}>#{item.id} · {item.kind}</option>)}</select></label>
          <label className="grid gap-1 text-xs">{t('channels.activeMonitor.model')}<select className="h-9 min-w-0 rounded-lg border border-[var(--hairline)] bg-background px-3" value={model} disabled={!channel} onChange={(event) => setModel(event.target.value)}><option value="">{t('channels.activeMonitor.choose')}</option>{models.map((value) => <option key={value} value={value}>{value}</option>)}</select></label>
          {[[interval, setInterval, 'interval', 300, 86400], [budget, setBudget, 'budget', 1, 288], [timeoutSecs, setTimeoutSecs, 'timeout', 1, 15], [alertAfter, setAlertAfter, 'alertAfter', 60, 3600]].map(([value, setter, key, min, max]) => <label key={String(key)} className="grid gap-1 text-xs">{t(`channels.activeMonitor.${key}`)}<Input size="sm" type="number" min={Number(min)} max={Number(max)} value={String(value)} onValueChange={(next) => (setter as (value: string) => void)(next)} /></label>)}
        </div>
        {channel && credentialsQuery.isPending ? <p role="status" className="text-xs text-muted-foreground">{t('channels.activeMonitor.loadingCredentials')}</p> : null}
        {channel && credentialsQuery.isError ? <p role="alert" className="text-xs text-danger">{t(isForbidden(credentialsQuery.error) ? 'channels.activeMonitor.forbidden' : 'channels.activeMonitor.credentialsError')}</p> : null}
        {channel && !credentialsQuery.isPending && credentials.length === 0 ? <p className="text-xs text-muted-foreground">{t('channels.activeMonitor.noCredentials')}</p> : null}
        <div className="flex flex-wrap items-center justify-between gap-2"><p className="text-[11px] leading-4 text-muted-foreground">{t('channels.activeMonitor.targetOptIn')}</p><Button size="sm" color="primary" isDisabled={!validTarget || saving} onPress={() => writeTarget({ channel_id: channel?.id ?? 0, credential_id: credential?.id ?? 0, model, enabled: false, interval_secs: Number(interval), daily_budget: Number(budget), timeout_secs: Number(timeoutSecs), alert_after_secs: Number(alertAfter), muted_until_epoch_seconds: null })}>{t('channels.activeMonitor.saveTarget')}</Button></div>
      </fieldset>
      {mutation.settings.isError || mutation.saveTarget.isError || mutation.deleteTarget.isError ? <PoolError message={[mutation.settings.error, mutation.saveTarget.error, mutation.deleteTarget.error].some(isForbidden) ? t('channels.activeMonitor.forbidden') : t('channels.activeMonitor.saveError')} /> : null}
    </div>
  </details>
}
