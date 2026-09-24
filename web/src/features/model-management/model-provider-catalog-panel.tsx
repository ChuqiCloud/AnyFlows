import { Pencil, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { jsonBodySerializer } from '@/lib/api/generated/client'
import { providerCatalog } from '@/components/brand/provider-catalog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'

type Provider = { provider_key: string; display_name: string; logo: string | null; aliases: string[]; enabled: boolean; sort_order: number; version: number }
type ProviderList = { providers: Provider[] }

const queryKey = ['admin-model-provider-catalog'] as const

export function ModelProviderCatalogPanel() {
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState<Provider | null>(null)
  const [formOpen, setFormOpen] = useState(false)
  const [form, setForm] = useState({ provider_key: '', display_name: '', logo: '', aliases: '', enabled: true, sort_order: '100' })
  const query = useQuery({ queryKey, queryFn: async ({ signal }) => (await apiClient.get<ProviderList>({ url: '/api/admin/model-provider-catalog', signal, throwOnError: true })).data.providers })
  const reset = () => { setEditing(null); setFormOpen(false); setForm({ provider_key: '', display_name: '', logo: '', aliases: '', enabled: true, sort_order: '100' }) }
  const create = () => { reset(); setFormOpen(true) }
  const edit = (provider: Provider) => { setEditing(provider); setFormOpen(true); setForm({ provider_key: provider.provider_key, display_name: provider.display_name, logo: provider.logo ?? '', aliases: provider.aliases.join(', '), enabled: provider.enabled, sort_order: String(provider.sort_order) }) }
  const save = async () => {
    const key = form.provider_key.trim().toLowerCase()
    if (!/^[a-z0-9][a-z0-9_-]{0,63}$/.test(key) || !form.display_name.trim()) return
    await apiClient.put<Provider>({ url: `/api/admin/model-provider-catalog/${encodeURIComponent(key)}`, body: { expected_version: editing?.version ?? 0, display_name: form.display_name.trim(), logo: form.logo.trim() || null, aliases: form.aliases.split(',').map((value) => value.trim()).filter(Boolean), enabled: form.enabled, sort_order: Math.max(0, Number(form.sort_order) || 0) }, bodySerializer: jsonBodySerializer.bodySerializer, throwOnError: true })
    await queryClient.invalidateQueries({ queryKey })
    await queryClient.invalidateQueries({ queryKey: ['model-provider-catalog'] })
    reset()
  }
  const remove = async (provider: Provider) => { if (!window.confirm(`删除 ${provider.display_name}？`)) return; await apiClient.delete({ url: `/api/admin/model-provider-catalog/${encodeURIComponent(provider.provider_key)}`, body: { expected_version: provider.version }, bodySerializer: jsonBodySerializer.bodySerializer, throwOnError: true }); await queryClient.invalidateQueries({ queryKey }); await queryClient.invalidateQueries({ queryKey: ['model-provider-catalog'] }) }
  const configured = new Map((query.data ?? []).map((provider) => [provider.provider_key, provider] as const))
  const providers = [...providerCatalog.map((provider, index) => configured.get(provider.id) ?? ({ provider_key: provider.id, display_name: provider.name, logo: provider.logo, aliases: [...(provider.aliases ?? [])], enabled: true, sort_order: index, version: 0 } satisfies Provider)), ...(query.data ?? []).filter((provider) => !providerCatalog.some((item) => item.id === provider.provider_key))]
    .sort((left, right) => left.sort_order - right.sort_order || left.display_name.localeCompare(right.display_name))

  return <section className="grid gap-3 rounded-xl border border-[var(--hairline)] bg-surface-1 p-4">
    <header className="flex flex-wrap items-start justify-between gap-3"><div><h3 className="text-sm font-semibold">模型厂商目录</h3><p className="mt-1 text-xs text-muted-foreground">统一管理模型和渠道中的厂商名称、Logo、别名与排序。</p></div><div className="flex gap-2"><Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />刷新</Button><Button type="button" size="sm" onClick={create}><Plus aria-hidden="true" />新增厂商</Button></div></header>
    {query.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">厂商目录读取失败，请稍后重试。</p> : null}
    <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">{providers.map((provider) => <article key={provider.provider_key} className="flex min-w-0 items-center gap-3 rounded-lg border border-[var(--hairline)] p-3"><span className="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-xs font-semibold">{provider.logo || provider.display_name.slice(0, 2).toUpperCase()}</span><div className="min-w-0 flex-1"><p className="truncate text-sm font-medium">{provider.display_name}</p><p className="truncate font-mono text-[0.6875rem] text-muted-foreground">{provider.provider_key}</p><p className="truncate text-[0.6875rem] text-muted-foreground">{provider.enabled ? '已启用' : '已停用'} · 排序 {provider.sort_order}</p></div><Button type="button" size="icon-sm" variant="ghost" aria-label="编辑" onClick={() => edit(provider)}><Pencil aria-hidden="true" /></Button>{provider.version > 0 ? <Button type="button" size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label="删除" onClick={() => void remove(provider)}><Trash2 aria-hidden="true" /></Button> : null}</article>)}</div>
    {formOpen ? <div className="grid gap-3 rounded-lg border border-dashed border-[var(--hairline)] p-3"><div className="grid gap-3 sm:grid-cols-2"><label className="grid gap-1 text-xs"><span>标识（小写）</span><Input value={form.provider_key} disabled={Boolean(editing)} placeholder="my-provider" onChange={(event) => setForm((current) => ({ ...current, provider_key: event.target.value }))} /></label><label className="grid gap-1 text-xs"><span>显示名称</span><Input value={form.display_name} placeholder="My Provider" onChange={(event) => setForm((current) => ({ ...current, display_name: event.target.value }))} /></label><label className="grid gap-1 text-xs"><span>Logo 标识</span><Input value={form.logo} placeholder="MyProvider" onChange={(event) => setForm((current) => ({ ...current, logo: event.target.value }))} /></label><label className="grid gap-1 text-xs"><span>别名（逗号分隔）</span><Input value={form.aliases} placeholder="alias, legacy-name" onChange={(event) => setForm((current) => ({ ...current, aliases: event.target.value }))} /></label><label className="grid gap-1 text-xs"><span>排序</span><Input type="number" min="0" value={form.sort_order} onChange={(event) => setForm((current) => ({ ...current, sort_order: event.target.value }))} /></label><label className="flex items-center gap-2 self-end text-xs"><Switch checked={form.enabled} onCheckedChange={(enabled) => setForm((current) => ({ ...current, enabled }))} />启用厂商</label></div><div className="flex justify-end gap-2"><Button type="button" size="sm" variant="secondary" onClick={reset}>取消</Button><Button type="button" size="sm" onClick={() => void save()}>保存</Button></div></div> : null}
  </section>
}
