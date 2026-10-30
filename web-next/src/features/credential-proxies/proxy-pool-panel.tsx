import { createContext, useContext, useId, useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'
import { Button as HeroButton, Input, Textarea as TextArea, Select, SelectItem, Skeleton, Switch, Drawer, DrawerContent, DrawerHeader } from '@heroui/react'
import type { ComponentProps } from 'react'
import { ProxyCheck, ProxySelection, ProxyCleanupControls } from './proxy-cleanup-controls'
import { useAdminRouteChannelCatalog } from '@/features/routes/route-api'
import { useAdminProxyPools, useProxyPoolAction, type AdminProxyPool, type ProxyPoolWrite } from './proxy-pool-api'

const PoolPortalContext = createContext<HTMLElement | null>(null)

export function ProxyPoolPanel() {
  const { t } = useTranslation()
  const query = useAdminProxyPools()
  const channels = useAdminRouteChannelCatalog(true)
  const action = useProxyPoolAction()
  const [editing, setEditing] = useState<AdminProxyPool | 'new'>()
  const [deleting, setDeleting] = useState<number>()
  const [selected, setSelected] = useState<number[]>([])
  const [scope, setScope] = useState('0')
  const [notice, setNotice] = useState('')
  const pools = query.data ?? []
  const bound = pools.find(pool => pool.channel_ids.includes(Number(scope)))
  const run = (kind: 'refresh' | 'check' | 'delete', id: number) => {
    action.mutate({ kind, id }, { onSuccess: () => setDeleting(undefined) })
  }
  return <section className="grid gap-5 rounded-xl border border-[var(--hairline)] bg-surface-1 p-5" aria-label={t('proxyPools.title')}>
    <header className="flex flex-wrap items-start justify-between gap-3">
      <div className="min-w-0"><h3 className="text-sm font-semibold">{t('proxyPools.title')}</h3><p className="mt-1 max-w-3xl text-xs leading-5 text-muted-foreground">{t('proxyPools.description')}</p></div>
      <Button onClick={() => { setNotice(''); setEditing('new') }}>{t('proxyPools.create')}</Button>
    </header>
    <div className="grid gap-2">
      <div><h4 className="text-xs font-medium">{t('proxyPools.groupRouting')}</h4><p className="mt-0.5 text-[0.6875rem] leading-4 text-muted-foreground">{t('proxyPools.groupRoutingHint')}</p></div>
      <div className="grid gap-3 rounded-lg bg-surface-2/50 p-3 sm:grid-cols-2">
        <Field label={t('proxyPools.scope')}><Choice label={t('proxyPools.scope')} value={scope} onChange={setScope} options={[{value:'0', label:t('proxyPools.global')}, ...(channels.data ?? []).map(channel => ({value:String(channel.id), label:`${channel.name} (#${channel.id})`}))]} disabled={channels.isPending || action.isPending} /></Field>
        <Field label={t('proxyPools.binding')}><Choice label={t('proxyPools.binding')} value={bound ? String(bound.id) : ''} onChange={value => action.mutate({kind:'bind',channelId:Number(scope),poolId:value ? Number(value) : null})} options={[{value:'',label:t('proxyPools.inherit')},...pools.map(pool => ({value:String(pool.id),label:pool.name}))]} disabled={query.isPending || query.isError || action.isPending} /></Field>
      </div>
    </div>
    <div className="grid gap-3 border-t border-[var(--hairline)] pt-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h4 className="text-xs font-medium">{t('proxyPools.groupPools')}</h4>
        <div className="flex flex-wrap items-center gap-3">
          <ProxySelection ids={pools.map(pool => pool.id)} selected={selected.filter(id => pools.some(pool => pool.id === id))} onChange={setSelected} />
          <ProxyCleanupControls target="pools" selected={selected.filter(id => pools.some(pool => pool.id === id))} disabled={query.isPending || query.isError} onDeleted={() => setSelected([])} />
        </div>
      </div>
      {channels.isError ? <p role="alert" className="text-xs text-destructive">{t('proxyPools.channelError')}</p> : null}
      {query.isPending ? <div className="grid gap-2" aria-label={t('proxyPools.loading')}>{[0, 1].map(item => <Skeleton key={item} className="h-24 rounded-lg" />)}</div>
        : query.isError ? <p role="alert" className="text-sm text-destructive">{t('proxyPools.loadError')}</p>
          : pools.length === 0 ? <p className="rounded-lg border border-dashed border-[var(--hairline)] py-6 text-center text-xs text-muted-foreground">{t('proxyPools.empty')}</p>
            : <div className="divide-y divide-[var(--hairline)] rounded-lg border border-[var(--hairline)]">{pools.map(pool => <article key={pool.id} className="grid gap-3 p-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div><ProxyCheck checked={selected.includes(pool.id)} onChange={checked => setSelected(ids => checked ? [...new Set([...ids,pool.id])].slice(0,256) : ids.filter(id => id !== pool.id))} label={t('proxyPools.selectPool',{id:pool.id})} /><h4 className="mt-2 text-sm font-semibold">{pool.name} <span className="font-normal text-muted-foreground">#{pool.id}</span></h4><p className="mt-1 text-xs text-muted-foreground">{t(`proxyPools.status.${pool.status}`, { defaultValue: pool.status })} · {t('proxyPools.count',{healthy:pool.nodes.filter(node => node.healthy).length,total:pool.nodes.length})} · {pool.subscribed ? pool.source_host ?? t('proxyPools.subscription') : t('proxyPools.manual')}</p><p className="mt-1 text-xs text-muted-foreground">{t('proxyPools.lastCheck')}: {pool.last_check_at ? new Date(pool.last_check_at * 1000).toLocaleString() : '—'}</p><p className="mt-1 text-xs text-muted-foreground">{t('proxyPools.every', {value: pool.interval_secs % 3600 === 0 ? pool.interval_secs / 3600 : pool.interval_secs % 60 === 0 ? pool.interval_secs / 60 : pool.interval_secs, unit:t(pool.interval_secs % 3600 === 0 ? 'proxyPools.hours' : pool.interval_secs % 60 === 0 ? 'proxyPools.minutes' : 'proxyPools.seconds')})}</p>{pool.subscribed ? <><p className="mt-1 text-xs text-muted-foreground">{t('proxyPools.lastRefresh')}: {pool.last_refresh_at ? new Date(pool.last_refresh_at * 1000).toLocaleString() : '—'}</p><p className="mt-1 text-xs text-muted-foreground">{t('proxyPools.lastAttempt')}: {pool.last_refresh_attempt_at ? new Date(pool.last_refresh_attempt_at * 1000).toLocaleString() : '—'}</p><p className="mt-1 text-xs text-muted-foreground">{t('proxyPools.nextRefresh')}: {pool.last_refresh_attempt_at ? new Date((pool.last_refresh_attempt_at + pool.interval_secs) * 1000).toLocaleString() : t('proxyPools.soon')}</p></> : null}</div>
                <div className="flex flex-wrap gap-2"><Button disabled={pool.busy || action.isPending} onClick={() => run('check',pool.id)}>{t('proxyPools.check')}</Button>{pool.subscribed ? <Button disabled={pool.busy || action.isPending} onClick={() => run('refresh',pool.id)}>{t('proxyPools.refresh')}</Button> : null}<Button disabled={action.isPending} onClick={() => setEditing(pool)}>{t('proxyPools.edit')}</Button><Button disabled={action.isPending} onClick={() => setDeleting(pool.id)}>{t('proxyPools.delete')}</Button></div>
              </div>
              {deleting === pool.id ? <div role="alert" className="flex flex-wrap items-center gap-3 text-xs"><span>{t('proxyPools.deleteConfirm')}</span><Button disabled={action.isPending} onClick={() => run('delete',pool.id)}>{t('proxyPools.confirm')}</Button><Button onClick={() => setDeleting(undefined)}>{t('proxyPools.cancel')}</Button></div> : null}
              <PoolNodes pool={pool} />
            </article>)}</div>}
    </div>
    {notice ? <p role="status" className="text-xs text-muted-foreground">{notice}</p> : null}
    {action.isError ? <p role="alert" className="text-xs text-destructive">{t('proxyPools.actionError')}</p> : null}
    {editing !== undefined ? <PoolEditor key={editing === 'new' ? 'new' : `${editing.id}-${editing.revision}`} pool={editing === 'new' ? undefined : editing} onClose={() => setEditing(undefined)} onSaved={(skipped,duplicates) => { setNotice(t('proxyPools.importResult',{skipped,duplicates})); setEditing(undefined) }} /> : null}
  </section>
}

function PoolNodes({pool}:{pool:AdminProxyPool}) {
  const { t } = useTranslation()
  const [filter,setFilter] = useState('all')
  const [search,setSearch] = useState('')
  const [selected,setSelected] = useState<number[]>([])
  const failed = (node: AdminProxyPool['nodes'][number]) => !node.healthy && node.checked_at !== null && node.latency_ms === null
  const nodes = pool.nodes.filter(node => (filter === 'all' || (filter === 'healthy' ? node.healthy : filter === 'failed' ? failed(node) : !node.healthy && !failed(node))) && `${node.host}:${node.port} ${node.remark}`.toLowerCase().includes(search.toLowerCase()))
  const selectedIds = selected.filter(id => nodes.some(node => node.id === id))
  return <div className="grid gap-2">
    <div className="flex flex-wrap gap-2"><Choice value={filter} onChange={setFilter} label={t('proxyPools.filter')} options={['all','healthy','failed','pending'].map(value => ({value,label:t(`proxyPools.${value}`)}))} /><Input aria-label={t('proxyPools.search')} placeholder={t('proxyPools.search')} value={search} onChange={event => setSearch(event.target.value)} /></div>
    <ProxySelection ids={nodes.map(node => node.id)} selected={selectedIds} onChange={setSelected} />
    <ProxyCleanupControls target="nodes" poolId={pool.id} revision={pool.revision} selected={selectedIds} disabled={pool.busy} onDeleted={() => setSelected([])} />
    <p className="text-xs leading-5 text-muted-foreground">{t('proxyPools.manualHint')}</p>
    {nodes.length ? <div className="max-h-64 overflow-auto"><table className="w-full text-left text-xs"><thead><tr className="border-b border-[var(--hairline)]"><th className="p-2">{t('proxyPools.node')}</th><th className="p-2">{t('proxyPools.health')}</th><th className="p-2">{t('proxyPools.remark')}</th></tr></thead><tbody>{nodes.map(node => <tr key={node.id} className="border-b border-[var(--hairline)]"><td className="p-2"><ProxyCheck checked={selectedIds.includes(node.id)} onChange={checked => setSelected(ids => checked ? [...new Set([...ids,node.id])].slice(0,256) : ids.filter(id => id !== node.id))} label={t('proxyPools.selectNode',{id:node.id})} /><span className="text-muted-foreground">#{node.id}</span> {node.scheme}://{node.host.includes(':') ? `[${node.host}]` : node.host}:{node.port}{node.password_configured ? ` · ${t('proxyPools.authenticated')}` : ''}</td><td className="p-2">{node.healthy ? t('proxyPools.healthy') : failed(node) ? t('proxyPools.failed') : t('proxyPools.pending')}{node.healthy && node.latency_ms !== null ? ` · ${node.latency_ms} ms` : ''}</td><td className="p-2">{node.remark || '—'}</td></tr>)}</tbody></table></div> : <p className="text-xs text-muted-foreground">{t('proxyPools.noNodes')}</p>}
  </div>
}

function PoolEditor({pool,onClose,onSaved}:{pool?:AdminProxyPool;onClose:()=>void;onSaved:(skipped:number,duplicates:number)=>void}) {
  const { t } = useTranslation()
  const action = useProxyPoolAction()
  const [portal, setPortal] = useState<HTMLDivElement | null>(null)
  const initialInterval = pool?.interval_secs ?? 900
  const initialUnit = initialInterval % 3600 === 0 ? 3600 : initialInterval % 60 === 0 ? 60 : 1
  const [intervalUnit,setIntervalUnit] = useState(String(initialUnit))
  const [form,setForm] = useState({name:pool?.name ?? '',subscribed:pool?.subscribed ?? false,source_url:'',interval_secs:String(initialInterval / initialUnit),default_scheme:pool?.default_scheme ?? 'socks5',trust_proxy_dns:pool?.trust_proxy_dns ?? false,probe_url:pool?.probe_url ?? 'https://www.gstatic.com/generate_204',content:''})
  const [invalid,setInvalid] = useState(false)
  const update = <K extends keyof typeof form>(key:K,value:(typeof form)[K]) => {setInvalid(false);setForm(current => ({...current,[key]:value}))}
  const submit = (event:FormEvent) => {
    event.preventDefault()
    const rawInterval = Number(form.interval_secs) * Number(intervalUnit)
    const interval = Math.round(rawInterval)
    if (!form.name.trim() || !Number.isFinite(rawInterval) || Math.abs(rawInterval - interval) > 0.000001 || interval < 60 || interval > 86400 || (!pool && form.subscribed && !form.source_url.trim())) {setInvalid(true);return}
    const body:ProxyPoolWrite = {...form,name:form.name.trim(),source_url:form.source_url.trim() || null,content:form.content.trim() || null,interval_secs:interval,revision:pool?.revision ?? null}
    action.mutate({kind:'save',id:pool?.id,body},{onSuccess:result => {if (result) onSaved(result.skipped,result.duplicates)}})
  }
  const readFile = async (file?:File) => {if (!file) return;if (file.size > 1_048_576) {setInvalid(true);return}try {update('content',await file.text())} catch {setInvalid(true)}}
  const title = t(pool ? 'proxyPools.edit' : 'proxyPools.create')
  return <Drawer isOpen placement="right" onOpenChange={open => !open && onClose()} classNames={{base:'w-full max-h-none sm:max-w-xl'}}><DrawerContent><div ref={setPortal} className="relative flex h-full min-h-0 flex-col"><PoolPortalContext.Provider value={portal}><DrawerHeader>{title}</DrawerHeader>
    <form className="grid min-h-0 gap-4 overflow-y-auto p-5" onSubmit={submit}>
      <Field label={t('proxyPools.name')}><Input aria-label={t('proxyPools.name')} autoFocus required maxLength={128} value={form.name} onChange={event => update('name',event.target.value)} /></Field>
      <Toggle checked={form.subscribed} onChange={value => update('subscribed',value)} label={t('proxyPools.subscription')} />
      {form.subscribed ? <Field label={t('proxyPools.sourceUrl')}><Input aria-label={t('proxyPools.sourceUrl')} type="url" autoComplete="off" value={form.source_url} placeholder={pool?.source_host ? t('proxyPools.keepUrl',{host:pool.source_host}) : 'https://example.com/proxies.txt'} onChange={event => update('source_url',event.target.value)} /></Field> : null}
      <div className="grid gap-3 sm:grid-cols-2"><Field label={t('proxyPools.interval')}><Input aria-label={t('proxyPools.interval')} required type="number" step="any" min={60 / Number(intervalUnit)} max={86400 / Number(intervalUnit)} value={form.interval_secs} onChange={event => update('interval_secs',event.target.value)} /><Choice label={t('proxyPools.intervalUnit')} value={intervalUnit} onChange={value => { const seconds = Number(form.interval_secs) * Number(intervalUnit); setIntervalUnit(value); update('interval_secs',String(seconds / Number(value))) }} options={[{value:'1',label:t('proxyPools.seconds')},{value:'60',label:t('proxyPools.minutes')},{value:'3600',label:t('proxyPools.hours')}]} /></Field><Field label={t('proxyPools.defaultScheme')}><Choice label={t('proxyPools.defaultScheme')} value={form.default_scheme} onChange={value => update('default_scheme',value)} options={['http','https','socks5','socks5h'].map(value => ({value,label:value.toUpperCase()}))} /></Field></div>
      <p className="text-xs text-muted-foreground">{t('proxyPools.intervalHint')}</p>
      <Field label={t('proxyPools.probeUrl')}><Input aria-label={t('proxyPools.probeUrl')} required type="url" value={form.probe_url} onChange={event => update('probe_url',event.target.value)} /></Field>
      <Toggle checked={form.trust_proxy_dns} onChange={value => update('trust_proxy_dns',value)} label={t('proxyPools.trustDns')} />
      <Field label={t('proxyPools.content')}><TextArea aria-label={t('proxyPools.content')} rows={7} autoComplete="off" value={form.content} onChange={event => update('content',event.target.value)} /></Field>
      <label className="grid gap-2 text-xs">{t('proxyPools.file')}<input type="file" accept=".txt,.csv,.json,text/plain,application/json" onChange={event => void readFile(event.target.files?.[0])} /></label>
      <p className="text-xs leading-5 text-muted-foreground">{t('proxyPools.formats')}</p>
      {invalid || action.isError ? <p role="alert" className="text-xs text-destructive">{t(invalid ? 'proxyPools.validationError' : 'proxyPools.actionError')}</p> : null}
      <div className="flex justify-end gap-2"><Button type="button" disabled={action.isPending} onClick={onClose}>{t('proxyPools.cancel')}</Button><Button type="submit" disabled={action.isPending}>{t('proxyPools.save')}</Button></div>
    </form>
  </PoolPortalContext.Provider></div></DrawerContent></Drawer>
}
function Field({label,children}:{label:string;children:ReactNode}) {return <label className="grid gap-2 text-xs font-medium">{label}{children}</label>}
function Button({disabled,...props}:Omit<ComponentProps<typeof HeroButton>,'isDisabled'> & {disabled?:boolean}) {return <HeroButton size="sm" variant="flat" isDisabled={disabled} {...props} />}
function Choice({value,onChange,options,disabled,label}:{value:string;onChange:(value:string)=>void;options:{value:string;label:string}[];disabled?:boolean;label?:string}) {
  const portal = useContext(PoolPortalContext)
  const labelId = useId()
  // HeroUI Select 只认 onSelectionChange，传 onChange 时选中值不会回写，触发器只能显示被截断的首项。
  return <div className="min-w-0"><span id={labelId} className="sr-only">{label ?? 'Select'}</span><Select aria-label={label ?? 'Select'} aria-labelledby={labelId} size="sm" isDisabled={disabled} placeholder={label} selectedKeys={new Set([value || 'none'])} popoverProps={{ className: 'min-w-44', ...(portal ? { portalContainer: portal } : {}) }} onSelectionChange={keys => { const key = Array.from(keys)[0]; if (typeof key === 'string') onChange(key === 'none' ? '' : key) }}>{options.map(option => <SelectItem key={option.value || 'none'}>{option.label}</SelectItem>)}</Select></div>
}
function Toggle({checked,onChange,label}:{checked:boolean;onChange:(value:boolean)=>void;label:string}) {return <Switch size="sm" isSelected={checked} onValueChange={onChange}>{label}</Switch>}
