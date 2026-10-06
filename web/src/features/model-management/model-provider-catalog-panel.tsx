import { ChevronLeft, ChevronRight, Network, Pencil, Plus, RefreshCw, Trash2 } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { resolveProviderLogo } from '@/components/brand/model-logos'
import { normalizeProvider, providerCatalog } from '@/components/brand/provider-catalog'
import { apiClient, ApiError } from '@/lib/api'
import { jsonBodySerializer } from '@/lib/api/generated/client'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Sheet, SheetContent, SheetDescription, SheetFooter, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Switch } from '@/components/ui/switch'

type Provider = { provider_key: string; display_name: string; logo: string | null; aliases: string[]; enabled: boolean; sort_order: number; version: number }
type ProviderList = { providers: Provider[] }
const queryKey = ['admin-model-provider-catalog'] as const
const managementSessionSecurity = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const
const PAGE_SIZE = 12

function ProviderLogo({ provider }: { provider: Provider }) {
  const configured = provider.logo?.trim()
  if (configured && /^https?:\/\//i.test(configured)) return <img className="size-7 object-contain" src={configured} alt="" loading="lazy" />
  const logo = resolveProviderLogo(configured, provider.provider_key, provider.display_name, ...provider.aliases)
  if (logo) {
    const Icon = logo.Icon
    return <Icon className="size-7" aria-hidden="true" />
  }
  return <Network className="size-7 text-muted-foreground" aria-hidden="true" />
}

export function ModelProviderCatalogPanel() {
  const queryClient = useQueryClient()
  const [editing, setEditing] = useState<Provider | null>(null)
  const [formOpen, setFormOpen] = useState(false)
  const [page, setPage] = useState(1)
  const [search, setSearch] = useState('')
  const [notice, setNotice] = useState('')
  const [form, setForm] = useState({ provider_key: '', display_name: '', logo: '', aliases: '', enabled: true, sort_order: '100' })
  const query = useQuery({ queryKey, queryFn: async ({ signal }) => ((await apiClient.get({ security: managementSessionSecurity, url: '/api/admin/model-provider-catalog', signal })) as unknown as { data: ProviderList }).data.providers })
  const reset = () => { setEditing(null); setFormOpen(false); setForm({ provider_key: '', display_name: '', logo: '', aliases: '', enabled: true, sort_order: '100' }) }
  const create = () => { reset(); saveMutation.reset(); setNotice(''); setFormOpen(true) }
  const edit = (provider: Provider) => { saveMutation.reset(); setNotice(''); setEditing(provider); setFormOpen(true); setForm({ provider_key: provider.provider_key, display_name: provider.display_name, logo: provider.logo ?? '', aliases: provider.aliases.join(', '), enabled: provider.enabled, sort_order: String(provider.sort_order) }) }
  const invalidate = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey }),
      queryClient.invalidateQueries({ queryKey: ['model-provider-catalog'] }),
    ])
  }
  const saveMutation = useMutation({
    mutationFn: async () => {
      const key = form.provider_key.trim().toLowerCase()
      if (!/^[a-z0-9][a-z0-9_-]{0,63}$/.test(key) || !form.display_name.trim()) throw new Error('invalid-provider')
      await apiClient.put({ security: managementSessionSecurity, url: `/api/admin/model-provider-catalog/${encodeURIComponent(key)}`, body: { expected_version: editing?.version ?? 0, display_name: form.display_name.trim(), logo: form.logo.trim() || null, aliases: form.aliases.split(',').map((value) => value.trim()).filter(Boolean), enabled: form.enabled, sort_order: Math.max(0, Number(form.sort_order) || 0) }, bodySerializer: jsonBodySerializer.bodySerializer })
    },
    onSuccess: async () => { await invalidate(); reset(); setNotice('厂商已保存。') },
  })
  const deleteMutation = useMutation({
    mutationFn: async (provider: Provider) => {
      await apiClient.delete({ security: managementSessionSecurity, url: `/api/admin/model-provider-catalog/${encodeURIComponent(provider.provider_key)}`, body: { expected_version: provider.version }, bodySerializer: jsonBodySerializer.bodySerializer })
    },
    onSuccess: async () => { await invalidate(); setNotice('厂商配置已删除。') },
  })
  const busy = saveMutation.isPending || deleteMutation.isPending
  const canManage = query.isSuccess && !query.isFetching && !busy
  const remove = (provider: Provider) => {
    if (!canManage || !window.confirm(`删除 ${provider.display_name}？`)) return
    setNotice('')
    deleteMutation.mutate(provider)
  }
  const writeError = deleteMutation.error
  const providers = useMemo(() => {
    const configured = new Map((query.data ?? []).map((provider) => [provider.provider_key, provider] as const))
    return [...providerCatalog.map((provider, index) => configured.get(provider.id) ?? ({ provider_key: provider.id, display_name: provider.name, logo: provider.logo, aliases: [...(provider.aliases ?? [])], enabled: true, sort_order: index, version: 0 } satisfies Provider)), ...(query.data ?? []).filter((provider) => !providerCatalog.some((item) => item.id === provider.provider_key))]
      .sort((left, right) => left.sort_order - right.sort_order || left.display_name.localeCompare(right.display_name))
  }, [query.data])
  const filtered = useMemo(() => {
    const value = normalizeProvider(search)
    return providers.filter((provider) => [provider.provider_key, provider.display_name, ...provider.aliases]
      .some((name) => normalizeProvider(name).includes(value)))
  }, [providers, search])
  const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
  const pageProviders = query.isSuccess ? filtered.slice((page - 1) * PAGE_SIZE, page * PAGE_SIZE) : []
  useEffect(() => { if (page > pageCount) setPage(pageCount) }, [page, pageCount])

  return <div className="grid gap-5">
    <header className="flex flex-wrap items-start justify-between gap-3"><div><h2 className="text-lg font-semibold">模型厂商目录</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">独立管理模型和渠道使用的厂商名称、Logo、别名与排序。</p></div><div className="flex gap-2"><Button type="button" size="sm" variant="secondary" disabled={query.isFetching} onClick={() => void query.refetch()}><RefreshCw className={query.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />刷新</Button><Button type="button" size="sm" disabled={!canManage} onClick={create}><Plus aria-hidden="true" />新增厂商</Button></div></header>
    {query.isError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">厂商目录读取失败，请稍后重试。</p> : null}
    <Input aria-label="搜索厂商" placeholder="搜索名称、标识或别名" value={search} onChange={(event) => { setSearch(event.target.value); setPage(1) }} />
    {query.isPending ? <p role="status" className="text-sm text-muted-foreground">正在读取厂商目录…</p> : null}
    {writeError ? <p role="alert" className="text-sm text-destructive">{writeError instanceof ApiError && writeError.status === 409 ? '厂商配置已更新，请刷新后重试。' : '操作失败，请检查输入内容后重试。'}</p> : null}
    {notice ? <p role="status" className="text-sm text-muted-foreground">{notice}</p> : null}
    {query.isSuccess && filtered.length === 0 ? <p className="text-sm text-muted-foreground">没有匹配的厂商。</p> : null}
    <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">{pageProviders.map((provider) => <article key={provider.provider_key} className="flex min-w-0 items-center gap-3 rounded-lg border border-[var(--hairline)] bg-surface-1 p-3"><span className="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2"><ProviderLogo provider={provider} /></span><div className="min-w-0 flex-1"><p className="truncate text-sm font-medium">{provider.display_name}</p><p className="truncate font-mono text-[0.6875rem] text-muted-foreground">{provider.provider_key}</p><p className="truncate text-[0.6875rem] text-muted-foreground">{provider.enabled ? '已启用' : '已停用'} · 排序 {provider.sort_order}</p></div><Button type="button" size="icon-sm" variant="ghost" aria-label="编辑" disabled={!canManage} onClick={() => edit(provider)}><Pencil aria-hidden="true" /></Button>{provider.version > 0 ? <Button type="button" size="icon-sm" variant="ghost" className="text-muted-foreground hover:text-destructive" aria-label="删除" disabled={!canManage} onClick={() => remove(provider)}><Trash2 aria-hidden="true" /></Button> : null}</article>)}</section>
    <div className="flex flex-wrap items-center justify-between gap-3"><span className="text-xs text-muted-foreground">第 {page} / {pageCount} 页，共 {filtered.length} 个厂商</span><div className="flex gap-2"><Button type="button" size="sm" variant="secondary" disabled={page <= 1} onClick={() => setPage((current) => current - 1)}><ChevronLeft aria-hidden="true" />上一页</Button><Button type="button" size="sm" variant="secondary" disabled={page >= pageCount} onClick={() => setPage((current) => current + 1)}>下一页<ChevronRight aria-hidden="true" /></Button></div></div>
    <Sheet open={formOpen} onOpenChange={(open) => { if (!open && !busy) reset() }}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-lg" aria-describedby="model-provider-editor-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{editing ? '编辑厂商' : '新增厂商'}</SheetTitle>
          <SheetDescription id="model-provider-editor-description">
            {editing ? editing.display_name : '配置模型和渠道使用的厂商信息。'}
          </SheetDescription>
        </SheetHeader>
        <form className="flex min-h-0 flex-1 flex-col" onSubmit={(event) => { event.preventDefault(); setNotice(''); saveMutation.mutate() }}>
          <div className="min-h-0 flex-1 overflow-y-auto p-4">
            <div className="grid gap-4">
              <label className="grid gap-1 text-sm"><span>标识（小写）</span><Input value={form.provider_key} disabled={Boolean(editing)} placeholder="my-provider" required onChange={(event) => setForm((current) => ({ ...current, provider_key: event.target.value }))} /></label>
              <label className="grid gap-1 text-sm"><span>显示名称</span><Input value={form.display_name} placeholder="My Provider" required onChange={(event) => setForm((current) => ({ ...current, display_name: event.target.value }))} /></label>
              <label className="grid gap-1 text-sm"><span>Logo 标识或 URL</span><Input value={form.logo} placeholder="OpenAI 或 https://..." onChange={(event) => setForm((current) => ({ ...current, logo: event.target.value }))} /></label>
              <label className="grid gap-1 text-sm"><span>别名（逗号分隔）</span><Input value={form.aliases} placeholder="alias, legacy-name" onChange={(event) => setForm((current) => ({ ...current, aliases: event.target.value }))} /></label>
              <label className="grid gap-1 text-sm"><span>排序</span><Input type="number" min="0" value={form.sort_order} onChange={(event) => setForm((current) => ({ ...current, sort_order: event.target.value }))} /></label>
              <label className="flex items-center gap-2 text-sm"><Switch checked={form.enabled} onCheckedChange={(enabled) => setForm((current) => ({ ...current, enabled }))} />启用厂商</label>
              {saveMutation.error ? <p role="alert" className="text-sm text-destructive">{saveMutation.error instanceof ApiError && saveMutation.error.status === 409 ? '厂商配置已更新，请刷新后重试。' : '保存失败，请检查输入内容后重试。'}</p> : null}
            </div>
          </div>
          <SheetFooter className="flex-row justify-end border-t border-[var(--hairline)]">
            <Button type="button" size="sm" variant="secondary" disabled={busy} onClick={reset}>取消</Button>
            <Button type="submit" size="sm" disabled={!canManage || !/^[a-z0-9][a-z0-9_-]{0,63}$/.test(form.provider_key.trim().toLowerCase()) || !form.display_name.trim()}>保存</Button>
          </SheetFooter>
        </form>
      </SheetContent>
    </Sheet>
  </div>
}
