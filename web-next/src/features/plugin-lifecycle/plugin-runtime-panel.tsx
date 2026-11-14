import { Button, Chip, Skeleton } from '@heroui/react'
import { Server } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { PluginDeploymentResponse } from '@/lib/api/generated/types.gen'
import { usePluginRuntimeOverview } from './plugin-lifecycle-api'

export function PluginRuntimePanel({ deployments }: { deployments: PluginDeploymentResponse[] }) {
  const { t } = useTranslation()
  const runtime = usePluginRuntimeOverview()
  const overview = runtime.data
  const liveNodes = overview?.nodes.filter((node) => node.live) ?? []

  return <section className="rounded-xl border border-[var(--hairline)] bg-card p-5" aria-label={t('pluginLifecycle.runtime.title')}>
    <div className="flex flex-wrap items-start justify-between gap-3">
      <div><h2 className="flex items-center gap-2 text-base font-semibold"><Server size={16} />{t('pluginLifecycle.runtime.title')}</h2><p className="mt-1 text-xs text-muted-foreground">{t('pluginLifecycle.runtime.description')}</p></div>
      <Button size="sm" variant="light" isDisabled={runtime.isFetching} onPress={() => void runtime.refetch()}>{t('pluginLifecycle.refresh')}</Button>
    </div>
    {runtime.isPending ? <Skeleton className="mt-4 h-20 rounded-lg" /> : runtime.isError ? <p role="alert" className="mt-4 text-sm text-danger">{t('pluginLifecycle.runtime.error')}</p> : overview ? <div className="mt-4 space-y-4">
      <div className="flex flex-wrap gap-2 text-xs"><Chip size="sm" variant="flat">{t('pluginLifecycle.runtime.liveNodes', { count: liveNodes.length })}</Chip><Chip size="sm" variant="flat">{t('pluginLifecycle.runtime.isolation', { mode: overview.isolation })}</Chip></div>
      {overview.nodes.length === 0 ? <p className="text-sm text-muted-foreground">{t('pluginLifecycle.runtime.noNodes')}</p> : <div className="grid gap-2 sm:grid-cols-2">{overview.nodes.map((node) => <div key={node.nodeId} className="min-w-0 rounded-lg border border-[var(--hairline)] p-3 text-xs"><div className="flex items-center justify-between gap-2"><strong className="truncate">{node.hostLabel || node.nodeId}</strong><Chip size="sm" variant="flat" color={node.live ? 'success' : 'default'}>{t(node.live ? 'pluginLifecycle.runtime.online' : 'pluginLifecycle.runtime.stale')}</Chip></div><p className="mt-1 break-all font-mono text-[10px] text-muted-foreground">{node.nodeId}</p><p className="mt-1 text-muted-foreground">{t('pluginLifecycle.runtime.lastSeen', { time: new Date(node.lastSeenAt * 1000).toLocaleString() })}</p></div>)}</div>}
      {deployments.map((deployment) => {
        const states = overview.states.filter((state) => state.pluginId === deployment.pluginId)
        const ready = liveNodes.filter((node) => states.some((state) => state.nodeId === node.nodeId && state.generation === deployment.generation && state.digest === deployment.artifact.digest && state.status === 'ready')).length
        return <div key={deployment.pluginId} className="rounded-lg border border-[var(--hairline)] p-3 text-xs">
          <div className="flex flex-wrap items-center justify-between gap-2"><strong>{deployment.pluginId}</strong><span className="text-muted-foreground">{deployment.enabled ? t('pluginLifecycle.runtime.readyNodes', { ready, total: liveNodes.length }) : t('pluginLifecycle.status.disabled')}</span></div>
          <div className="mt-2 space-y-1">{liveNodes.map((node) => {
            const state = states.find((item) => item.nodeId === node.nodeId && item.generation === deployment.generation && item.digest === deployment.artifact.digest)
            const status = state?.status ?? 'pending'
            return <div key={node.nodeId} className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1"><span className="min-w-0 truncate text-muted-foreground">{node.hostLabel || node.nodeId}</span><span className={status === 'failed' ? 'text-danger' : 'text-muted-foreground'}>{t(`pluginLifecycle.runtime.states.${status}`, { defaultValue: status })}{state?.isolation ? ` · ${t(`pluginLifecycle.runtime.isolationModes.${state.isolation}`, { defaultValue: state.isolation })}` : ''}{state?.errorCode ? ` · ${state.errorCode}` : ''}</span></div>
          })}</div>
        </div>
      })}
    </div> : null}
  </section>
}
