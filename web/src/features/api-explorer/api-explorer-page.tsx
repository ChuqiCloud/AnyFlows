import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Braces, ChevronRight, CircleHelp, LoaderCircle, Play, Search, ShieldCheck } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { PublicSiteHeader } from '@/components/layout/public-site-header'
import { SiteFooter } from '@/components/layout/site-footer'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import {
  fetchApiExplorerCatalog,
  fetchApiExplorerDetail,
  operationParameterNames,
  operationPathParameterNames,
  runApiExplorerOperation,
  type ApiExplorerCatalog,
  type ApiExplorerDetail,
  type ApiExplorerOperation,
  type ApiExplorerRunResult,
} from './api-explorer-api'

function methodClass(method: string) {
  if (method === 'GET') return 'text-emerald-500'
  if (method === 'POST') return 'text-sky-500'
  if (method === 'DELETE') return 'text-rose-500'
  return 'text-amber-500'
}

function formatResponse(body: string) {
  try {
    return JSON.stringify(JSON.parse(body), null, 2)
  } catch {
    return body
  }
}

export function ApiExplorerPage() {
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')
  const [search, setSearch] = useState('')
  const [tag, setTag] = useState('')
  const [page, setPage] = useState(1)
  const [catalog, setCatalog] = useState<ApiExplorerCatalog>()
  const [catalogError, setCatalogError] = useState(false)
  const [selected, setSelected] = useState<ApiExplorerOperation>()
  const [detail, setDetail] = useState<ApiExplorerDetail>()
  const [detailError, setDetailError] = useState(false)
  const [apiKey, setApiKey] = useState('')
  const [pathParams, setPathParams] = useState<Record<string, string>>({})
  const [queryParams, setQueryParams] = useState<Record<string, string>>({})
  const [body, setBody] = useState('')
  const [result, setResult] = useState<ApiExplorerRunResult>()
  const [runError, setRunError] = useState<string>()
  const [running, setRunning] = useState(false)

  useEffect(() => {
    const controller = new AbortController()
    setCatalogError(false)
    const timeout = window.setTimeout(() => void fetchApiExplorerCatalog(search, tag, page, controller.signal)
      .then((nextCatalog) => {
        setCatalog((current) => page > 1 && current
          ? { ...nextCatalog, operations: [...current.operations, ...nextCatalog.operations] }
          : nextCatalog)
        setSelected((current) => page > 1 ? current ?? nextCatalog.operations[0] : nextCatalog.operations.find((item) => item.operation_id === current?.operation_id) ?? nextCatalog.operations[0])
      })
      .catch((error: unknown) => {
        if (!(error instanceof DOMException && error.name === 'AbortError')) setCatalogError(true)
      }), 180)
    return () => { window.clearTimeout(timeout); controller.abort() }
  }, [search, tag, page])

  useEffect(() => {
    if (!selected) {
      setDetail(undefined)
      return
    }
    const controller = new AbortController()
    setDetailError(false)
    setResult(undefined)
    void fetchApiExplorerDetail(selected.operation_id, controller.signal)
      .then((nextDetail) => {
        setDetail(nextDetail)
        setPathParams({})
        setQueryParams({})
        setBody(nextDetail.operation.method === 'POST' ? '{\n  \n}' : '')
      })
      .catch((error: unknown) => {
        if (!(error instanceof DOMException && error.name === 'AbortError')) setDetailError(true)
      })
    return () => controller.abort()
  }, [selected])

  const parameters = useMemo(() => operationParameterNames(detail), [detail])
  const pathNames = useMemo(() => selected ? operationPathParameterNames(selected) : [], [selected])

  const run = async () => {
    if (!selected) return
    setRunning(true)
    setRunError(undefined)
    setResult(undefined)
    try {
      setResult(await runApiExplorerOperation(selected, { pathParams, queryParams, body, apiKey }))
    } catch (error) {
      setRunError(error instanceof Error ? error.message : t('apiExplorer.errors.run'))
    } finally {
      setRunning(false)
    }
  }

  return (
    <div className="min-h-dvh bg-background text-foreground">
      <PublicSiteHeader siteName={siteName} logoUrl={site?.brand.logo_url} />
      <main className="px-4 pt-20 pb-8 md:px-6">
        <div className="mx-auto max-w-[1440px]">
          <header className="mb-5 flex flex-col gap-2">
            <div className="flex items-center gap-2 text-brand"><Braces className="size-5" aria-hidden="true" /><span className="text-xs font-medium uppercase tracking-widest">API</span></div>
            <h1 className="text-2xl font-semibold tracking-tight md:text-3xl">{t('apiExplorer.title')}</h1>
            <p className="max-w-3xl text-sm leading-6 text-muted-foreground">{t('apiExplorer.subtitle')}</p>
          </header>

          <div className="grid gap-4 lg:grid-cols-[18rem_minmax(0,1fr)_minmax(20rem,0.8fr)]">
            <aside className="flex min-h-[32rem] flex-col overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1">
              <div className="border-b border-[var(--hairline)] p-3">
                <label className="relative block">
                  <Search className="pointer-events-none absolute top-2 left-2.5 size-3.5 text-muted-foreground" aria-hidden="true" />
                  <Input value={search} onChange={(event) => { setSearch(event.target.value); setPage(1) }} placeholder={t('apiExplorer.search')} className="pl-8" />
                </label>
                <select value={tag} onChange={(event) => { setTag(event.target.value); setPage(1) }} className="mt-2 h-8 w-full rounded-lg border border-input bg-transparent px-2 text-xs text-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring/50">
                  <option value="">{t('apiExplorer.allTags')}</option>
                  {catalog?.tags.map((item) => <option key={item} value={item}>{item}</option>)}
                </select>
              </div>
              <div className="border-b border-[var(--hairline)] px-3 py-2 text-[0.7rem] text-muted-foreground">
                {catalog ? t('apiExplorer.count', { count: catalog.total }) : t('apiExplorer.loading')}
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto p-2">
                {catalogError ? <p className="p-3 text-xs text-destructive">{t('apiExplorer.errors.catalog')}</p> : null}
                {!catalogError && catalog?.operations.length === 0 ? <p className="p-3 text-xs text-muted-foreground">{t('apiExplorer.empty')}</p> : null}
                {catalog?.operations.map((operation) => (
                  <button key={operation.operation_id} type="button" onClick={() => setSelected(operation)} className={`group flex w-full items-start gap-2 rounded-lg px-2.5 py-2 text-left transition-colors hover:bg-surface-2 ${selected?.operation_id === operation.operation_id ? 'bg-surface-2' : ''}`}>
                    <span className={`mt-0.5 w-11 shrink-0 font-mono text-[0.65rem] font-semibold ${methodClass(operation.method)}`}>{operation.method}</span>
                    <span className="min-w-0 flex-1"><span className="block truncate text-xs font-medium">{operation.summary}</span><span className="mt-0.5 block truncate font-mono text-[0.65rem] text-muted-foreground">{operation.path}</span></span>
                    <ChevronRight className="mt-1 size-3 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" aria-hidden="true" />
                  </button>
                ))}
                {catalog?.has_more ? <Button type="button" size="sm" variant="secondary" className="mt-2 w-full" onClick={() => setPage((current) => current + 1)}>{t('apiExplorer.loadMore')}</Button> : null}
              </div>
            </aside>

            <section className="min-w-0 rounded-xl border border-[var(--hairline)] bg-surface-1">
              {selected && detail ? <>
                <div className="border-b border-[var(--hairline)] p-5">
                  <div className="flex flex-wrap items-center gap-2"><span className={`font-mono text-xs font-semibold ${methodClass(selected.method)}`}>{selected.method}</span><code className="rounded bg-surface-2 px-2 py-1 text-xs">{selected.path}</code><span className="rounded-full border border-[var(--hairline)] px-2 py-0.5 text-[0.65rem] text-muted-foreground">{selected.auth_mode === 'api_key' ? t('apiExplorer.auth.apiKey') : selected.auth_mode === 'session' ? t('apiExplorer.auth.session') : selected.auth_mode === 'special' ? t('apiExplorer.auth.special') : t('apiExplorer.auth.none')}</span></div>
                  <h2 className="mt-4 text-lg font-semibold">{selected.summary}</h2>
                  {selected.description ? <p className="mt-2 whitespace-pre-wrap text-sm leading-6 text-muted-foreground">{selected.description}</p> : null}
                </div>
                <div className="p-5">
                  {parameters.length > 0 || pathNames.length > 0 ? <div className="mb-5"><h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">{t('apiExplorer.parameters')}</h3><div className="grid gap-2 sm:grid-cols-2">{[...new Map([...pathNames.map((name) => ({ name, location: 'path' as const, required: true })), ...parameters].map((item) => [`${item.location}:${item.name}`, item])).values()].map((parameter) => <label key={`${parameter.location}:${parameter.name}`} className="grid gap-1 text-xs"><span className="text-muted-foreground">{parameter.name} <span className="font-mono text-[0.65rem]">({parameter.location})</span>{parameter.required ? ' *' : ''}</span><Input value={(parameter.location === 'path' ? pathParams : queryParams)[parameter.name] ?? ''} onChange={(event) => (parameter.location === 'path' ? setPathParams : setQueryParams)((current) => ({ ...current, [parameter.name]: event.target.value }))} placeholder={parameter.required ? t('apiExplorer.required') : t('apiExplorer.optional')} /></label>)}</div></div> : null}
                  {selected.auth_mode === 'api_key' ? <label className="mb-5 grid gap-1 text-xs"><span className="text-muted-foreground">{t('apiExplorer.apiKey')}</span><Input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} autoComplete="off" placeholder="sk-…" /></label> : null}
                  {selected.method !== 'GET' && selected.method !== 'HEAD' ? <label className="grid gap-1 text-xs"><span className="text-muted-foreground">{t('apiExplorer.requestBody')}</span><textarea value={body} onChange={(event) => setBody(event.target.value)} spellCheck={false} className="min-h-40 w-full rounded-lg border border-input bg-transparent p-2.5 font-mono text-xs outline-none focus-visible:ring-2 focus-visible:ring-ring/50" placeholder="{}" /></label> : null}
                  <details className="mt-5 rounded-lg border border-[var(--hairline)]"><summary className="cursor-pointer px-3 py-2 text-xs font-medium">{t('apiExplorer.contract')}</summary><pre className="max-h-72 overflow-auto border-t border-[var(--hairline)] bg-surface-2 p-3 font-mono text-[0.68rem] leading-5">{JSON.stringify({ requestBody: detail.openapi.requestBody, responses: detail.openapi.responses }, null, 2)}</pre></details>
                  {detailError ? <p className="mt-4 text-xs text-destructive">{t('apiExplorer.errors.detail')}</p> : null}
                </div>
              </> : <div className="flex min-h-[32rem] items-center justify-center p-8 text-sm text-muted-foreground"><LoaderCircle className="mr-2 size-4 animate-spin" aria-hidden="true" />{t('apiExplorer.loading')}</div>}
            </section>

            <section className="flex min-h-[32rem] flex-col rounded-xl border border-[var(--hairline)] bg-surface-1">
              <div className="border-b border-[var(--hairline)] p-4"><div className="flex items-center gap-2"><Play className="size-4 text-brand" aria-hidden="true" /><h2 className="text-sm font-semibold">{t('apiExplorer.debug.title')}</h2></div><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('apiExplorer.debug.subtitle')}</p></div>
              <div className="flex-1 p-4">
                {selected?.debuggable ? <>
                  <Button type="button" size="sm" className="w-full" disabled={running} onClick={() => void run()}>{running ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <Play aria-hidden="true" />}{running ? t('apiExplorer.debug.running') : t('apiExplorer.debug.send')}</Button>
                  {runError ? <p role="alert" className="mt-3 rounded-lg border border-destructive/25 bg-destructive/8 p-3 text-xs text-destructive">{runError}</p> : null}
                  {result ? <div className="mt-4 overflow-hidden rounded-lg border border-[var(--hairline)]"><div className="flex flex-wrap items-center gap-3 border-b border-[var(--hairline)] px-3 py-2 text-xs"><strong className={result.ok ? 'text-emerald-500' : 'text-destructive'}>{result.status}</strong><span className="text-muted-foreground">{result.duration_ms} ms</span>{result.request_id ? <span className="truncate font-mono text-[0.65rem] text-muted-foreground">{result.request_id}</span> : null}</div><pre className="max-h-[28rem] overflow-auto whitespace-pre-wrap break-words bg-surface-2 p-3 font-mono text-[0.7rem] leading-5">{formatResponse(result.body)}</pre></div> : <div className="mt-4 flex min-h-40 flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-[var(--hairline)] p-5 text-center text-xs text-muted-foreground"><CircleHelp className="size-5" aria-hidden="true" />{t('apiExplorer.debug.empty')}</div>}
                </> : <div className="flex min-h-40 flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-[var(--hairline)] p-5 text-center text-xs text-muted-foreground"><ShieldCheck className="size-5" aria-hidden="true" />{t('apiExplorer.debug.disabled')}</div>}
              </div>
            </section>
          </div>
        </div>
      </main>
      <SiteFooter siteName={siteName} logoUrl={site?.brand.logo_url} navigation={site?.navigation} />
    </div>
  )
}
