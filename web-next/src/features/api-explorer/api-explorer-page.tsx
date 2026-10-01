import { useEffect, useMemo, useState } from 'react'
import { Braces, ChevronRight, CircleHelp, LoaderCircle, Play, Search, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button, Card, CardBody, Chip, Input, Select, SelectItem, Spinner, Textarea } from '@heroui/react'

import { PublicSiteHeader } from '@/components/layout/public-site-header'
import { SiteFooter } from '@/components/layout/site-footer'
import { AnnouncementStrip } from '@/features/announcements/announcement-strip'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { fetchApiExplorerCatalog, fetchApiExplorerDetail, operationParameterNames, runApiExplorerOperation, type ApiExplorerCatalog, type ApiExplorerDetail, type ApiExplorerOperation, type ApiExplorerRunResult } from './api-explorer-api'

function methodColor(method: string) {
  return method === 'GET' ? 'success' : method === 'POST' ? 'primary' : method === 'DELETE' ? 'danger' : 'warning'
}

function pretty(value: string) {
  try { return JSON.stringify(JSON.parse(value), null, 2) } catch { return value }
}

export function ApiExplorerPage() {
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const site = siteQuery.data
  const [search, setSearch] = useState('')
  const [tag, setTag] = useState('')
  const [page, setPage] = useState(1)
  const [catalog, setCatalog] = useState<ApiExplorerCatalog>()
  const [selected, setSelected] = useState<ApiExplorerOperation>()
  const [detail, setDetail] = useState<ApiExplorerDetail>()
  const [pathParams, setPathParams] = useState<Record<string, string>>({})
  const [queryParams, setQueryParams] = useState<Record<string, string>>({})
  const [apiKey, setApiKey] = useState('')
  const [body, setBody] = useState('')
  const [result, setResult] = useState<ApiExplorerRunResult>()
  const [error, setError] = useState<string>()
  const [loading, setLoading] = useState(false)
  const [catalogError, setCatalogError] = useState(false)
  const [catalogRetry, setCatalogRetry] = useState(0)
  const [detailError, setDetailError] = useState(false)
  const [detailRetry, setDetailRetry] = useState(0)
  const [catalogLoading, setCatalogLoading] = useState(false)
  const [detailLoading, setDetailLoading] = useState(false)

  useEffect(() => {
    const controller = new AbortController()
    setCatalogError(false)
    if (page === 1) { setCatalog(undefined); setSelected(undefined) }
    setCatalogLoading(true)
    const timeout = window.setTimeout(() => void fetchApiExplorerCatalog(search, tag, page, controller.signal).then((next) => { setCatalog((current) => page > 1 && current ? { ...next, operations: [...current.operations, ...next.operations] } : next); setSelected((current) => page > 1 ? current ?? next.operations[0] : next.operations.find((item) => item.operation_id === current?.operation_id) ?? next.operations[0]) }).catch((reason) => { if (reason?.name !== 'AbortError') setCatalogError(true) }).finally(() => setCatalogLoading(false)), 180)
    return () => { window.clearTimeout(timeout); controller.abort() }
  }, [search, tag, page, catalogRetry])

  useEffect(() => {
    if (!selected) { setDetail(undefined); return }
    const controller = new AbortController()
    setDetail(undefined)
    setDetailError(false)
    setDetailLoading(true)
    void fetchApiExplorerDetail(selected.operation_id, controller.signal).then((next) => { setDetail(next); setPathParams({}); setQueryParams({}); setBody(next.operation.method === 'POST' ? '{\n  \n}' : ''); setResult(undefined) }).catch((reason) => { if (reason?.name !== 'AbortError') setDetailError(true) }).finally(() => setDetailLoading(false))
    return () => controller.abort()
  }, [selected, detailRetry])

  const parameters = useMemo(() => operationParameterNames(detail), [detail])
  const run = async () => {
    if (!selected) return
    setLoading(true); setError(undefined); setResult(undefined)
    try { setResult(await runApiExplorerOperation(selected, { pathParams, queryParams, body, apiKey })) } catch (reason) { setError(reason instanceof Error ? reason.message : t('apiExplorer.errors.run')) } finally { setLoading(false) }
  }

  const authLabel = selected?.auth_mode === 'api_key' ? t('apiExplorer.auth.apiKey') : selected?.auth_mode === 'session' ? t('apiExplorer.auth.session') : selected?.auth_mode === 'special' ? t('apiExplorer.auth.special') : t('apiExplorer.auth.none')
  return <div className="min-h-dvh bg-background text-foreground"><PublicSiteHeader siteName={site?.site_name ?? t('brand.name')} logoUrl={site?.brand.logo_url} /><div className="pt-14"><AnnouncementStrip /></div><main className="px-4 pb-8 pt-4 md:px-6"><div className="mx-auto max-w-[1440px]"><header className="mb-5"><div className="flex items-center gap-2 text-primary"><Braces className="size-5" aria-hidden="true" /><span className="text-xs font-medium uppercase tracking-widest">API</span></div><h1 className="mt-2 text-2xl font-semibold">{t('apiExplorer.title')}</h1><p className="mt-2 max-w-3xl text-small text-default-500">{t('apiExplorer.subtitle')}</p></header><div className="grid gap-4 lg:grid-cols-[18rem_minmax(0,1fr)_minmax(20rem,0.8fr)]"><Card shadow="sm" className="min-h-[32rem]"><CardBody className="gap-3 p-3"><Input size="sm" startContent={<Search className="size-4 text-default-400" aria-hidden="true" />} value={search} onValueChange={(value) => { setSearch(value); setPage(1) }} placeholder={t('apiExplorer.search')} /><Select size="sm" selectedKeys={tag ? [tag] : []} onSelectionChange={(keys) => { setTag(String(Array.from(keys)[0] ?? '')); setPage(1) }} placeholder={t('apiExplorer.allTags')}>{(catalog?.tags ?? []).map((item) => <SelectItem key={item}>{item}</SelectItem>)}</Select><p className="border-b border-divider pb-2 text-tiny text-default-400">{catalog ? t('apiExplorer.count', { count: catalog.total }) : catalogError ? t('apiExplorer.errors.catalog') : t('apiExplorer.loading')}</p><div className="min-h-0 flex-1 overflow-y-auto">{catalogError ? <div className="grid min-h-40 place-items-center gap-2 p-4 text-center text-tiny text-danger"><span>{t('apiExplorer.errors.catalog')}</span><Button size="sm" variant="flat" onPress={() => setCatalogRetry((current) => current + 1)}>{t('apiExplorer.actions.retry', 'Retry')}</Button></div> : catalogLoading && !catalog ? <div className="flex min-h-40 items-center justify-center gap-2 text-small text-default-400"><Spinner size="sm" />{t('apiExplorer.loading')}</div> : catalog?.operations.length ? <>{catalog.operations.map((operation) => <button type="button" key={operation.operation_id} onClick={() => setSelected(operation)} className={`group flex w-full items-start gap-2 rounded-lg px-2 py-2 text-left hover:bg-default-100 ${selected?.operation_id === operation.operation_id ? 'bg-default-100' : ''}`}><Chip size="sm" variant="flat" color={methodColor(operation.method)} className="mt-0.5 w-12 justify-center">{operation.method}</Chip><span className="min-w-0 flex-1"><span className="block truncate text-small font-medium">{operation.summary}</span><span className="mt-0.5 block truncate font-mono text-tiny text-default-400">{operation.path}</span></span><ChevronRight className="mt-1 size-3 text-default-400 opacity-0 group-hover:opacity-100" aria-hidden="true" /></button>)}{catalog.has_more ? <Button size="sm" variant="flat" className="mt-2 w-full" onPress={() => setPage((current) => current + 1)}>{t('apiExplorer.loadMore')}</Button> : null}</> : <div className="flex min-h-40 items-center justify-center text-small text-default-400">{t('apiExplorer.empty')}</div>}</div></CardBody></Card><Card shadow="sm" className="min-h-[32rem]"><CardBody className="p-0">{detailError ? <div className="flex min-h-[32rem] flex-col items-center justify-center gap-3 p-5 text-center text-small text-danger"><span>{t('apiExplorer.errors.detail')}</span><Button size="sm" variant="flat" onPress={() => setDetailRetry((current) => current + 1)}>{t('apiExplorer.actions.retry', 'Retry')}</Button></div> : selected && detail ? <><div className="border-b border-divider p-5"><div className="flex flex-wrap items-center gap-2"><Chip size="sm" color={methodColor(selected.method)} variant="flat">{selected.method}</Chip><code className="rounded bg-default-100 px-2 py-1 text-tiny">{selected.path}</code><Chip size="sm" variant="bordered">{authLabel}</Chip></div><h2 className="mt-4 text-lg font-semibold">{selected.summary}</h2>{selected.description ? <p className="mt-2 whitespace-pre-wrap text-small leading-6 text-default-500">{selected.description}</p> : null}</div><div className="grid gap-4 p-5">{parameters.length ? <div className="grid gap-3 sm:grid-cols-2">{parameters.map((parameter) => <Input key={`${parameter.location}:${parameter.name}`} size="sm" label={`${parameter.name} (${parameter.location})`} isRequired={parameter.required} value={(parameter.location === 'path' ? pathParams : queryParams)[parameter.name] ?? ''} onValueChange={(value) => (parameter.location === 'path' ? setPathParams : setQueryParams)((current) => ({ ...current, [parameter.name]: value }))} />)}</div> : null}{selected.auth_mode === 'api_key' ? <Input size="sm" type="password" label={t('apiExplorer.apiKey')} value={apiKey} onValueChange={setApiKey} autoComplete="off" /> : null}{selected.method !== 'GET' && selected.method !== 'HEAD' ? <Textarea minRows={7} label={t('apiExplorer.requestBody')} value={body} onValueChange={setBody} /> : null}<details className="rounded-lg border border-divider"><summary className="cursor-pointer px-3 py-2 text-small font-medium">{t('apiExplorer.contract')}</summary><pre className="max-h-72 overflow-auto border-t border-divider bg-default-50 p-3 font-mono text-tiny leading-5">{JSON.stringify({ requestBody: detail.openapi.requestBody, responses: detail.openapi.responses }, null, 2)}</pre></details></div></> : detailLoading || selected ? <div className="flex min-h-[32rem] items-center justify-center gap-2 text-small text-default-400"><Spinner size="sm" />{t('apiExplorer.loading')}</div> : <div className="flex min-h-[32rem] items-center justify-center text-small text-default-400">{t('apiExplorer.empty')}</div>}</CardBody></Card><Card shadow="sm" className="min-h-[32rem]"><CardBody><div className="flex items-center gap-2"><Play className="size-4 text-primary" aria-hidden="true" /><h2 className="text-small font-semibold">{t('apiExplorer.debug.title')}</h2></div><p className="mt-1 text-tiny text-default-500">{t('apiExplorer.debug.subtitle')}</p>{selected?.debuggable ? <><Button className="mt-4" color="primary" size="sm" isDisabled={loading} onPress={() => void run()}>{loading ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Play className="size-4" aria-hidden="true" />}{loading ? t('apiExplorer.debug.running') : t('apiExplorer.debug.send')}</Button>{error ? <p className="mt-3 text-tiny text-danger">{error}</p> : null}{result ? <div className="mt-4 overflow-hidden rounded-lg border border-divider"><div className="flex gap-3 border-b border-divider px-3 py-2 text-tiny"><strong className={result.ok ? 'text-success' : 'text-danger'}>{result.status}</strong><span className="text-default-400">{result.duration_ms} ms</span></div><pre className="max-h-[28rem] overflow-auto whitespace-pre-wrap break-words bg-default-50 p-3 font-mono text-tiny leading-5">{pretty(result.body)}</pre></div> : <div className="mt-4 flex min-h-40 flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-divider p-5 text-center text-tiny text-default-400"><CircleHelp className="size-5" aria-hidden="true" />{t('apiExplorer.debug.empty')}</div>}</> : <div className="mt-4 flex min-h-40 flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-divider p-5 text-center text-tiny text-default-400"><ShieldCheck className="size-5" aria-hidden="true" />{t('apiExplorer.debug.disabled')}</div>}</CardBody></Card></div></div></main><SiteFooter siteName={site?.site_name ?? t('brand.name')} logoUrl={site?.brand.logo_url} navigation={site?.navigation} /></div>
}
