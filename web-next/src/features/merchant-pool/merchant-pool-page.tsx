import { useEffect, useState } from 'react'
import { Modal, ModalBody, ModalContent, ModalHeader } from '@heroui/react'
import { ArrowUpRight, GitBranch, ShieldCheck, Store } from 'lucide-react'
import { Link, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useMerchantCatalog, useMerchantModelProviders, useMerchantPools, useMerchantStatistics, useMerchantTimeline } from './merchant-pool-api'
import { MerchantHealth, MerchantModelPrice } from './merchant-catalog-card'
import { MerchantCatalogTable } from './merchant-catalog-table'
import { MerchantMarketFiltersPanel, type MerchantMarketFilters } from './merchant-market-filters'
import { parseMerchantProviders, serializeMerchantProviders } from './merchant-model-provider'
import { merchantTextProtocol, parsePoolRatio, poolTimestamp, positiveInteger } from './merchant-pool-model'
import { PoolButton as Button, PoolEmpty, PoolError, PoolHeader, PoolLoading, PoolNavigation, PoolPager, PoolSelect } from './merchant-pool-ui'

const initialFilters = { search: '', status: '', sort: 'name', protocol: '', streaming: '', capability: '', minRatio: '', maxRatio: '' }

export function MerchantPoolPage() {
  const { t, i18n } = useTranslation()
  const [params, setParams] = useSearchParams()
  const pools = useMerchantPools()
  const poolId = positiveInteger(params.get('pool') ?? '') ?? pools.data?.items[0]?.id
  const model = params.get('model') ?? ''
  const providers = parseMerchantProviders(params.get('provider'))
  const provider = serializeMerchantProviders(providers)
  const providerFacets = useMerchantModelProviders(poolId)
  const [filters, setFilters] = useState(initialFilters)
  const { search, status, sort, protocol, streaming, capability, minRatio, maxRatio } = filters
  const minimum = minRatio === '' ? undefined : parsePoolRatio(minRatio)
  const maximum = maxRatio === '' ? undefined : parsePoolRatio(maxRatio)
  const invalidPriceRange = (minRatio !== '' && minimum === undefined) || (maxRatio !== '' && maximum === undefined) || (minimum !== undefined && maximum !== undefined && minimum > maximum)
  const [page, setPage] = useState(0)
  const [windowSeconds, setWindowSeconds] = useState('86400')
  const [compared, setCompared] = useState<number[]>([])
  const [comparisonOpen, setComparisonOpen] = useState(false)
  const catalog = useMerchantCatalog(poolId, { model, provider: provider || undefined, search, status, sort: model ? sort : 'name', page: page + 1, protocol: protocol || undefined, capability: capability || undefined, min_ratio_micros: invalidPriceRange ? undefined : minimum, max_ratio_micros: invalidPriceRange ? undefined : maximum, streaming: streaming === '' ? undefined : streaming === 'true', window_seconds: Number(windowSeconds) })
  const stats = useMerchantStatistics(poolId, model, Number(windowSeconds), { protocol: protocol || undefined, streaming: streaming === '' ? undefined : streaming === 'true' })
  const timeline = useMerchantTimeline(poolId, model, Number(windowSeconds), { protocol: protocol || undefined, streaming: streaming === '' ? undefined : streaming === 'true' })
  const items = catalog.data?.items ?? []
  const statByService = new Map(stats.data?.items.map((item) => [item.service_id, item]))
  const timelineByService = new Map(timeline.data?.items.map((item) => [item.service_id, item]))
  const selected = items.filter((item) => compared.includes(item.id))
  const routeUrl = `/console/merchant-pool/routes${poolId ? `?pool=${poolId}` : ''}`
  useEffect(() => { setPage(0); setCompared([]) }, [poolId, model, provider, search, status, sort, protocol, capability, minRatio, maxRatio, streaming, windowSeconds])
  useEffect(() => setCompared([]), [page])
  const setParam = (name: string, value: string) => { const next = new URLSearchParams(params); if (value) next.set(name, value); else next.delete(name); setParams(next, { replace: true }) }
  const changeFilters = (patch: Partial<MerchantMarketFilters>) => {
    const { model: nextModel, providers: nextProviders, ...rest } = patch
    if (nextModel !== undefined || nextProviders !== undefined) {
      const next = new URLSearchParams(params)
      if (nextModel !== undefined) { if (nextModel) next.set('model', nextModel); else next.delete('model') }
      if (nextProviders !== undefined) { const value = serializeMerchantProviders(nextProviders); if (value) next.set('provider', value); else next.delete('provider') }
      setParams(next, { replace: true })
    }
    setFilters((current) => ({ ...current, ...rest, ...(rest.protocol && !merchantTextProtocol(rest.protocol) ? { streaming: '' } : {}) }))
  }
  const resetFilters = () => { setFilters(initialFilters); const next = new URLSearchParams(params); next.delete('model'); next.delete('provider'); setParams(next, { replace: true }); setPage(0); setCompared([]) }
  const refresh = () => { void pools.refetch(); if (poolId !== undefined) void Promise.all([providerFacets.refetch(), catalog.refetch(), stats.refetch(), timeline.refetch()]) }
  const refreshing = pools.isFetching || providerFacets.isFetching || catalog.isFetching || stats.isFetching || timeline.isFetching
  return <div className="flex min-w-0 flex-col gap-4">
    <PoolHeader title={t('merchantPool.market.title')} description={t('merchantPool.market.subtitle')} refreshing={refreshing} onRefresh={refresh}><Link className="inline-flex h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-xs font-medium text-primary-foreground shadow-sm transition-opacity hover:opacity-90 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary" to={routeUrl}><GitBranch className="size-3.5" aria-hidden="true" />{t('merchantPool.createRoute')}</Link></PoolHeader>
    <PoolNavigation />
    <MerchantMarketFiltersPanel pools={pools.data?.items ?? []} poolId={poolId} loading={pools.isPending} providers={providerFacets.data?.providers ?? []} providersLoading={poolId !== undefined && providerFacets.isPending} providersError={providerFacets.isError} onRetryProviders={() => void providerFacets.refetch()} filters={{ ...filters, model, providers }} invalidPriceRange={invalidPriceRange} onPoolChange={(value) => setParam('pool', value)} onChange={changeFilters} onReset={resetFilters} />
    {pools.isError ? <PoolError onRetry={() => void pools.refetch()} /> : null}
    <section aria-label={t('merchantPool.market.routing')} className="flex min-w-0 flex-wrap items-center gap-3 rounded-xl border border-primary/15 bg-primary/4 px-3 py-3 sm:px-4">
      <div className="flex size-9 shrink-0 items-center justify-center rounded-lg border border-primary/15 bg-primary/8 text-primary"><GitBranch className="size-4" aria-hidden="true" /></div>
      <div className="min-w-0 flex-1"><h3 className="text-xs font-semibold">{t('merchantPool.market.routingTitle')}</h3><p className="mt-1 text-[11px] leading-5 text-muted-foreground">{t('merchantPool.market.routingHint')}</p></div>
      <Link to={routeUrl} className="ml-auto inline-flex shrink-0 items-center gap-1.5 rounded-lg border border-primary/20 bg-surface-1 px-3 py-2 text-xs font-medium text-primary transition-colors hover:bg-primary/10">{t('merchantPool.market.configureRouting')}<ArrowUpRight className="size-3.5" aria-hidden="true" /></Link>
    </section>
    <section aria-label={t('merchantPool.market.results')} className="flex min-w-0 flex-col gap-2.5">
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-3 px-0.5">
        <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1.5"><h3 className="flex items-center gap-2 text-sm font-semibold"><Store className="size-4 text-primary" aria-hidden="true" />{t('merchantPool.catalogCount', { count: catalog.data?.total ?? 0 })}</h3><span className="inline-flex items-center gap-1 text-[11px] text-muted-foreground"><ShieldCheck className="size-3.5 text-primary" aria-hidden="true" />{t('merchantPool.market.reviewed')}</span></div>
        <div className="flex flex-wrap items-center gap-2"><div className="w-28"><PoolSelect label={t('merchantPool.statWindow')} allowEmpty={false} value={windowSeconds} onChange={setWindowSeconds} options={['900', '3600', '86400', '604800'].map((value) => ({ value, label: t(`merchantPool.windows.${value}`) }))} /></div><Button size="sm" variant="bordered" isDisabled={!model || selected.length < 2} onPress={() => setComparisonOpen(true)}>{t('merchantPool.compare', { count: selected.length })}</Button></div>
      </div>
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-2 px-0.5 text-[11px] text-muted-foreground"><span>{model ? t('merchantPool.market.modelQuote', { model }) : t('merchantPool.selectModelToCompare')}</span>{stats.data ? <span>{t('merchantPool.statsAsOf', { value: poolTimestamp(stats.data.as_of, i18n.language) })}</span> : null}</div>
      {stats.isError || timeline.isError ? <PoolError message={t('merchantPool.statsError')} onRetry={() => void Promise.all([stats.refetch(), timeline.refetch()])} /> : null}
      {pools.isPending ? <PoolLoading /> : !poolId ? <PoolEmpty>{t('merchantPool.noAccessiblePool')}</PoolEmpty> : catalog.isPending ? <PoolLoading /> : catalog.isError ? <PoolError onRetry={() => void catalog.refetch()} /> : items.length === 0 ? <PoolEmpty /> : <MerchantCatalogTable services={items} model={model} statistics={statByService} timelines={timelineByService} compared={compared} onCompare={(serviceId) => setCompared((current) => current.includes(serviceId) ? current.filter((id) => id !== serviceId) : current.length < 4 ? [...current, serviceId] : current)} />}
      {catalog.data ? <PoolPager page={page} hasMore={(page + 1) * catalog.data.page_size < catalog.data.total} loading={catalog.isFetching} onChange={setPage} /> : null}
    </section>
    <details className="rounded-lg border border-[var(--hairline)] bg-surface-1 px-3 py-2.5 text-[11px] leading-5 text-muted-foreground"><summary className="w-fit cursor-pointer font-medium">{t('merchantPool.statisticsAndBilling')}</summary><p className="mt-2">{t('merchantPool.statisticsHint')}</p><p>{t('merchantPool.meteringHint')}</p><p>{t('merchantPool.market.deliveryHint')}</p></details>
    <Modal isOpen={comparisonOpen} onOpenChange={setComparisonOpen} size="5xl" scrollBehavior="inside"><ModalContent className="min-w-0 bg-surface-1 text-foreground"><ModalHeader>{t('merchantPool.compareTitle', { model })}</ModalHeader><ModalBody className="pb-5"><div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">{selected.map((service) => { const offer = service.models.find((item) => item.model === model); return <article key={service.id} className="grid content-start gap-4 rounded-lg border border-[var(--hairline)] p-3"><div><h3 className="text-sm font-semibold">{service.merchant_name}</h3><p className="mt-1 text-xs text-muted-foreground">{service.name}</p>{service.description ? <p className="mt-2 line-clamp-2 break-words text-[11px] leading-5 text-muted-foreground" title={service.description}>{service.description}</p> : null}</div>{offer ? <MerchantModelPrice offer={offer} /> : null}<MerchantHealth ttftApplicable={merchantTextProtocol(service.protocol) && Boolean(offer?.supports_streaming) && streaming !== 'false'} stats={statByService.get(service.id)} /><Link className="text-xs text-primary hover:underline" to={`/console/merchant-pool/routes?pool=${poolId}&merchant=${service.merchant_id}`}>{t('merchantPool.addToRoute')}</Link></article> })}</div></ModalBody></ModalContent></Modal>
  </div>
}
