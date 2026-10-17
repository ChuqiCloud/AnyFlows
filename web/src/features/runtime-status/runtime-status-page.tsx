import { Activity, Database, RefreshCw, Server } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { ApiError } from '@/lib/api'
import { useRuntimeStatus } from './runtime-status-api'
import { resourcePath, type RuntimeDependency } from './runtime-status-model'
import './runtime-status.css'

export function RuntimeStatusPage() {
  const { t, i18n } = useTranslation()
  const query = useRuntimeStatus()
  const denied = query.error instanceof ApiError && (query.error.status === 401 || query.error.status === 403)
  const data = denied ? undefined : query.data
  const [selectedId, setSelectedId] = useState<string>()
  const [page, setPage] = useState(0)
  const locale = i18n.resolvedLanguage?.startsWith('zh') ? 'zh-CN' : 'en-US'
  const number = (value: number | null | undefined, digits = 0) => value == null ? '—' : new Intl.NumberFormat(locale, { maximumFractionDigits: digits }).format(value)
  const date = (value: number | null | undefined) => value ? new Date(value * 1000).toLocaleString(locale) : '—'
  const bytes = (value: number | null | undefined) => {
    if (value == null) return '—'
    const unit = value >= 1024 ** 3 ? 3 : value >= 1024 ** 2 ? 2 : value >= 1024 ? 1 : 0
    return `${number(value / 1024 ** unit, 1)} ${['B', 'KiB', 'MiB', 'GiB'][unit]}`
  }
  const selected = data?.nodes.find(node => node.instance_id === (selectedId ?? data.local_instance_id)) ?? data?.nodes[0]
  const pages = Math.max(1, Math.ceil((data?.nodes.length ?? 0) / 6))
  const currentPage = Math.min(page, pages - 1)
  const cache = data?.prompt_cache
  const cacheAvailable = cache?.sampled_at != null
  const stateLabel = (status: string) => t(`runtimeStatus.states.${status}`, { defaultValue: t('runtimeStatus.states.unknown') })
  const status = (value: string) => <span className="runtime-badge" data-state={value}>{stateLabel(value)}</span>
  const dependency = (label: string, value: RuntimeDependency) => <div className="runtime-dependency"><span>{label}<strong>{value.kind}</strong></span><span>{status(value.status)}<small>{value.latency_ms === null ? '—' : `${number(value.latency_ms)} ms`}</small></span></div>

  return <div className="runtime-page" data-runtime-theme="classic">
    <header className="runtime-header">
      <div><span className="runtime-eyebrow"><Activity size={14} aria-hidden="true" />{t('runtimeStatus.eyebrow')}</span><h2>{t('runtimeStatus.title')}</h2><p>{t('runtimeStatus.subtitle')}</p></div>
      <div className="runtime-refresh"><span>{t('runtimeStatus.updated', { time: date(data?.sampled_at) })}</span><Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw size={14} className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />{t('runtimeStatus.refresh')}</Button></div>
    </header>
    {query.isError && <div className="runtime-notice runtime-error" role="alert">{t(data ? 'runtimeStatus.refreshError' : 'runtimeStatus.error')}<Button type="button" size="sm" variant="secondary" onClick={() => void query.refetch()}>{t('runtimeStatus.retry')}</Button></div>}
    {query.isPending ? <div className="runtime-skeleton" role="status" aria-label={t('runtimeStatus.loading')}><div className="runtime-summary">{[0, 1, 2, 3].map(key => <Skeleton key={key} className="h-28 rounded-xl" />)}</div><Skeleton className="h-56 rounded-xl" /><Skeleton className="h-64 rounded-xl" /></div> : data && <>
      <div className="runtime-summary">
        <Metric icon={<Server size={17} />} label={t('runtimeStatus.onlineNodes')} value={`${number(data.nodes.filter(node => node.status === 'online').length)} / ${number(data.nodes.length)}`} detail={stateLabel(data.cluster_status)} />
        <Metric icon={<Activity size={17} />} label={t('runtimeStatus.processCpu')} value={selected?.cpu_cores == null ? '—' : `${number(selected.cpu_cores, 2)} ${t('runtimeStatus.cores')}`} detail={t('runtimeStatus.capacity', { value: number(selected?.cpu_capacity_cores, 2) })} />
        <Metric icon={<Server size={17} />} label={t('runtimeStatus.processMemory')} value={bytes(selected?.memory_bytes)} detail={selected?.node_id ?? '—'} />
        <Metric icon={<Database size={17} />} label={t('runtimeStatus.database')} value={selected?.database.kind ?? '—'} detail={selected ? stateLabel(selected.database.status) : '—'} />
      </div>
      {data.cluster_status !== 'healthy' && <div className="runtime-notice">{t(data.cluster_status === 'local' ? 'runtimeStatus.localHint' : 'runtimeStatus.clusterHint', { time: date(data.cluster_observed_at) })}</div>}
      <section className="runtime-section" aria-labelledby="runtime-nodes">
        <div className="runtime-section-title"><div><h3 id="runtime-nodes">{t('runtimeStatus.nodes')}</h3><p>{t('runtimeStatus.nodesHint')}</p></div><span>{t('runtimeStatus.nodeCount', { count: data.nodes.length })}</span></div>
        <div className="runtime-nodes">{data.nodes.slice(currentPage * 6, currentPage * 6 + 6).map(node => <button key={node.instance_id} type="button" className="runtime-node" data-state={node.status} aria-pressed={node.instance_id === selected?.instance_id} onClick={() => setSelectedId(node.instance_id)}>
          <span className="runtime-node-heading"><strong>{node.node_id}</strong>{status(node.status)}</span>
          <span className="runtime-node-version">{node.os} · {node.version}{node.instance_id === data.local_instance_id ? ` · ${t('runtimeStatus.currentNode')}` : ''}</span>
          <span className="runtime-node-values"><span>CPU<strong>{node.cpu_cores == null ? '—' : `${number(node.cpu_cores, 2)} ${t('runtimeStatus.cores')}`}</strong></span><span>{t('runtimeStatus.processMemory')}<strong>{bytes(node.memory_bytes)}</strong></span></span>
          <span className="runtime-node-footer"><span>{node.database.kind} · {stateLabel(node.database.status)}</span><span>Redis · {stateLabel(node.redis.status)}</span></span>
          <span className="runtime-node-version">{t('runtimeStatus.updated', { time: date(node.sampled_at) })}</span>
        </button>)}</div>
        {pages > 1 && <nav className="runtime-pagination" aria-label={t('runtimeStatus.pagination')}><Button size="sm" variant="secondary" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>{t('runtimeStatus.previous')}</Button><span>{currentPage + 1} / {pages}</span><Button size="sm" variant="secondary" disabled={currentPage + 1 >= pages} onClick={() => setPage(currentPage + 1)}>{t('runtimeStatus.next')}</Button></nav>}
      </section>
      {selected && <section className="runtime-section" aria-labelledby="runtime-resources">
        <div className="runtime-section-title"><div><h3 id="runtime-resources">{t('runtimeStatus.resources')} · {selected.node_id}</h3><p>{t('runtimeStatus.uptime', { hours: number(Math.floor(selected.uptime_seconds / 3600)), minutes: number(Math.floor(selected.uptime_seconds % 3600 / 60)) })} · {t('runtimeStatus.started', { time: date(selected.started_at) })}</p></div>{status(selected.status)}</div>
        <div className="runtime-details"><div className="runtime-dependencies">{dependency(t('runtimeStatus.database'), selected.database)}{dependency('Redis', selected.redis)}<dl className="runtime-memory"><div><dt>{t('runtimeStatus.hostMemory')}</dt><dd>{bytes(selected.host_memory_used_bytes)} / {bytes(selected.host_memory_bytes)}</dd></div><div><dt>{t('runtimeStatus.containerMemory')}</dt><dd>{bytes(selected.container_memory_bytes)} / {bytes(selected.container_memory_limit_bytes)}</dd></div></dl><p className="runtime-muted">{t('runtimeStatus.resourceHint')}</p></div><div className="runtime-trends">
          <Trend label={t('runtimeStatus.processCpu')} values={selected.history.map(point => point.cpu_cores)} value={selected.cpu_cores == null ? '—' : `${number(selected.cpu_cores, 2)} ${t('runtimeStatus.cores')}`} color="#3b82f6" empty={t('runtimeStatus.noSamples')} />
          <Trend label={t('runtimeStatus.processMemory')} values={selected.history.map(point => point.memory_bytes)} value={bytes(selected.memory_bytes)} color="#10b981" empty={t('runtimeStatus.noSamples')} />
          <p className="runtime-trend-period">{date(selected.history[0]?.sampled_at)} — {date(selected.history.at(-1)?.sampled_at)}</p>
        </div></div>
      </section>}
      {cache && <section className="runtime-section runtime-cache" aria-labelledby="runtime-cache">
        <div className="runtime-section-title"><div><h3 id="runtime-cache">{t('runtimeStatus.cacheTitle')}</h3><p>{cacheAvailable ? `${date(cache.period_start)} — ${date(cache.period_end)}` : t('runtimeStatus.cacheWaiting')}</p></div>{status(cache.status)}</div>
        <div className="runtime-cache-metrics">{[
          ['inputTokens', cache.input_tokens], ['cacheRead', cache.cache_read_tokens], ['cacheWrite', cache.cache_write_tokens], ['positiveRequests', cache.positive_cache_requests],
        ].map(([key, value]) => <div key={key}><span>{t(`runtimeStatus.${key}`)}</span><strong>{cacheAvailable ? number(value as number) : '—'}</strong></div>)}</div>
        <div className="runtime-cache-source"><span>{t('runtimeStatus.upstreamRequests')}: {cacheAvailable ? number(cache.upstream_requests) : '—'}</span><span>{t('runtimeStatus.estimatedRequests')}: {cacheAvailable ? number(cache.estimated_requests) : '—'}</span><span>{t('runtimeStatus.updated', { time: date(cache.sampled_at) })}</span></div>
        <p className="runtime-notice">{t('runtimeStatus.cacheHint')}</p>
      </section>}
    </>}
  </div>
}

function Metric({ label, value, detail, icon }: { label: string; value: string; detail: string; icon: ReactNode }) {
  return <div className="runtime-metric"><span>{icon}{label}</span><strong>{value}</strong><small>{detail}</small></div>
}

function Trend({ label, values, value, color, empty }: { label: string; values: (number | null)[]; value: string; color: string; empty: string }) {
  const hasSamples = values.some(value => value !== null)
  return <div className="runtime-trend"><div><span>{label}</span><strong>{value}</strong></div>{hasSamples ? <svg viewBox="0 0 600 110" preserveAspectRatio="none" role="img" aria-label={`${label}: ${value}`}><title>{`${label}: ${value}`}</title>{[25, 60, 100].map(y => <line key={y} x1="0" x2="600" y1={y} y2={y} stroke="currentColor" strokeOpacity="0.08" />)}<path d={resourcePath(values)} stroke={color} strokeWidth="2.5" fill="none" vectorEffect="non-scaling-stroke" strokeLinecap="round" /></svg> : <p>{empty}</p>}</div>
}
