import { Activity, KeyRound, Network, Pencil, Timer } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
import { ProviderLabel } from '@/components/brand/provider-picker'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { AdminChannel, AdminChannelProbeResponse } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'

export type ProbeDisplay = (AdminChannelProbeResponse & { failed?: false }) | { failed: true }

type ChannelTableProps = {
  channels: AdminChannel[]
  probeResults: Record<number, ProbeDisplay>
  probingId?: number
  onCredentials: (channel: AdminChannel) => void
  onEdit: (channel: AdminChannel) => void
  onProbe: (channelId: number) => void
}

const statusStyles = {
  enabled: 'bg-success/10 text-success',
  disabled: 'bg-surface-2 text-muted-foreground',
  auto_disabled: 'bg-warning/10 text-warning',
} as const

function StatusBadge({ status }: { status: AdminChannel['status'] }) {
  const { t } = useTranslation()
  return <Badge className={cn('border-transparent', statusStyles[status])}>{t(`channels.status.${status}`)}</Badge>
}

function ProtocolBadge({ protocol }: { protocol: AdminChannel['protocol'] }) {
  const { t } = useTranslation()
  const tone = protocol === 'anthropic'
    ? 'bg-warning/10 text-warning'
    : protocol === 'gemini'
      ? 'bg-brand/10 text-brand'
      : protocol === 'openai_responses'
        ? 'bg-success/10 text-success'
        : protocol === 'openai_embeddings'
          ? 'bg-surface-2 text-foreground'
          : protocol === 'openai_images'
            ? 'bg-warning/10 text-warning'
            : protocol === 'openai_audio'
              ? 'bg-primary/10 text-primary'
              : protocol === 'openai_speech'
                ? 'bg-brand/10 text-brand'
                : protocol === 'jina_rerank'
                  ? 'bg-success/10 text-success'
                  : protocol === 'cohere_rerank'
                    ? 'bg-warning/10 text-warning'
                    : protocol === 'xai_video'
                      ? 'bg-info/10 text-info'
                  : 'bg-info/10 text-info'
  return <Badge className={cn('border-transparent', tone)}>{t(`channels.protocol.${protocol}`)}</Badge>
}

function ProbeState({ probing, value }: { probing?: boolean; value?: ProbeDisplay }) {
  const { t } = useTranslation()
  if (probing) return <AiActivity active label={t('channels.actions.probe')} size="compact" />
  if (!value) return <span className="text-muted-foreground">{t('channels.probe.notTested')}</span>
  if (value.failed) return <span className="text-destructive">{t('channels.probe.failed')}</span>
  const tone = value.status === 'healthy' ? 'text-success' : value.status === 'timeout' ? 'text-warning' : 'text-destructive'
  return (
    <span className={cn('inline-flex items-center gap-1.5', tone)}>
      <Timer className="size-3.5" aria-hidden="true" />
      {t(`channels.probe.${value.status}`)} · {value.latency_ms} ms
    </span>
  )
}

function RoutingBadges({ channel }: { channel: AdminChannel }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap gap-1">
      <Badge className="border-transparent bg-info/10 text-info">{t('channels.values.models', { count: channel.models.length })}</Badge>
      <Badge className="border-transparent bg-success/10 text-success">{t('channels.values.groups', { count: channel.group_ids.length })}</Badge>
      {channel.responses_websocket_enabled ? (
        <Badge className="border-transparent bg-brand/10 text-brand">{t('channels.values.responsesWebsocket')}</Badge>
      ) : null}
      {channel.client_simulation_profile ? (
        <Badge className="border-transparent bg-warning/10 text-warning">
          {t('channels.values.clientSimulation')}
        </Badge>
      ) : null}
      {channel.client_simulation_body_profile ? (
        <Badge className="border-transparent bg-destructive/10 text-destructive">
          {t('channels.values.clientSimulationBody')}
        </Badge>
      ) : null}
    </div>
  )
}

function ScheduleBadges({ channel }: { channel: AdminChannel }) {
  const { t } = useTranslation()
  return (
    <div className="flex flex-wrap gap-1 tabular-nums">
      <Badge>{t('channels.values.priority', { value: channel.priority })}</Badge>
      <Badge>{t('channels.values.weight', { value: channel.weight })}</Badge>
      {channel.pool_mode ? (
        <Badge className="border-transparent bg-warning/10 text-warning">{t('channels.values.poolMode')}</Badge>
      ) : null}
      <Badge className="text-muted-foreground">
        {channel.timeout_secs == null
          ? t('channels.values.inheritTimeout')
          : t('channels.values.timeout', { value: channel.timeout_secs })}
      </Badge>
    </div>
  )
}

function RowActions({ channel, probing, onCredentials, onEdit, onProbe }: {
  channel: AdminChannel
  probing: boolean
  onCredentials: (channel: AdminChannel) => void
  onEdit: (channel: AdminChannel) => void
  onProbe: (channelId: number) => void
}) {
  const { t } = useTranslation()
  return (
    <div className="flex items-center justify-end gap-1">
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" size="icon-sm" variant="ghost" className="size-10 md:size-8" disabled={probing} aria-label={t('channels.actions.probe')} onClick={() => onProbe(channel.id)}>
            <Activity aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t('channels.actions.probe')}</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" size="icon-sm" variant="ghost" className="size-10 md:size-8" aria-label={t('channels.actions.credentials')} onClick={() => onCredentials(channel)}>
            <KeyRound aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t('channels.actions.credentials')}</TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button type="button" size="icon-sm" variant="ghost" className="size-10 md:size-8" aria-label={t('channels.actions.edit')} onClick={() => onEdit(channel)}>
            <Pencil aria-hidden="true" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t('channels.actions.edit')}</TooltipContent>
      </Tooltip>
    </div>
  )
}

export function ChannelTable({ channels, probeResults, probingId, onCredentials, onEdit, onProbe }: ChannelTableProps) {
  const { t } = useTranslation()
  if (channels.length === 0) {
    return (
      <div className="grid min-h-64 place-items-center border-y border-[var(--hairline)] py-10 text-center">
        <div className="max-w-xs">
          <div className="mx-auto grid size-10 place-items-center rounded-lg bg-surface-2 text-muted-foreground"><Network className="size-4" aria-hidden="true" /></div>
          <h2 className="mt-3 text-sm font-semibold">{t('channels.empty.title')}</h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('channels.empty.body')}</p>
        </div>
      </div>
    )
  }

  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 md:block">
        <table className="w-full table-fixed text-left text-xs">
          <thead className="bg-surface-2/60 text-muted-foreground">
            <tr><th className="w-[31%] px-3 py-2 font-medium">{t('channels.columns.channel')}</th><th className="w-[21%] px-3 py-2 font-medium">{t('channels.columns.routing')}</th><th className="w-[16%] px-3 py-2 font-medium">{t('channels.columns.schedule')}</th><th className="w-[20%] px-3 py-2 font-medium">{t('channels.columns.health')}</th><th className="w-[12%] px-3 py-2"><span className="sr-only">{t('channels.columns.actions')}</span></th></tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)]">
            {channels.map((channel) => (
              <tr key={channel.id} className="hover:bg-surface-2/35">
                <td className="px-3 py-2.5"><div className="flex items-center gap-2"><span className="truncate font-medium">{channel.name}</span><StatusBadge status={channel.status} /><ProtocolBadge protocol={channel.protocol} />{channel.tag ? <Badge className="max-w-28 truncate">{channel.tag}</Badge> : null}</div><div className="mt-1 truncate text-[0.6875rem] text-muted-foreground">#{channel.id} · {channel.base_url ?? t('channels.values.defaultUrl')}</div><div className="mt-1 min-w-0 text-[0.6875rem] text-muted-foreground"><ProviderLabel provider={channel.provider ?? channel.type} /></div></td>
                <td className="px-3 py-2.5"><RoutingBadges channel={channel} /></td>
                <td className="px-3 py-2.5"><ScheduleBadges channel={channel} /></td>
                <td className="px-3 py-2.5 text-[0.6875rem]"><ProbeState probing={probingId === channel.id} value={probeResults[channel.id]} /></td>
                <td className="px-2 py-2.5"><RowActions channel={channel} probing={probingId === channel.id} onCredentials={onCredentials} onEdit={onEdit} onProbe={onProbe} /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="grid gap-2 md:hidden">
        {channels.map((channel) => (
          <article key={channel.id} className="min-w-0 rounded-xl border border-[var(--hairline)] bg-surface-1 p-3">
            <div className="flex items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><h2 className="truncate text-sm font-semibold">{channel.name}</h2><StatusBadge status={channel.status} /><ProtocolBadge protocol={channel.protocol} />{channel.tag ? <Badge className="max-w-32 truncate">{channel.tag}</Badge> : null}</div><p className="mt-1 truncate text-[0.6875rem] text-muted-foreground">#{channel.id} · {channel.base_url ?? t('channels.values.defaultUrl')}</p><div className="mt-1 min-w-0 text-xs text-muted-foreground"><ProviderLabel provider={channel.provider ?? channel.type} /></div></div><RowActions channel={channel} probing={probingId === channel.id} onCredentials={onCredentials} onEdit={onEdit} onProbe={onProbe} /></div>
            <div className="mt-3 grid gap-2 border-t border-[var(--hairline)] pt-3"><RoutingBadges channel={channel} /><ScheduleBadges channel={channel} /></div>
            <div className="mt-3 text-xs"><ProbeState probing={probingId === channel.id} value={probeResults[channel.id]} /></div>
          </article>
        ))}
      </div>
    </>
  )
}
