import { merchantProtocols, merchantTextProtocol } from './merchant-pool-model'
import { useEffect, useState } from 'react'
import { Input, Modal, ModalBody, ModalContent, ModalHeader } from '@heroui/react'
import { Link, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useMerchantCatalog, useMerchantPools, useMerchantStatistics, useMerchantTimeline } from './merchant-pool-api'
import { MerchantHealth, MerchantModelPrice } from './merchant-catalog-card'
import { MerchantCatalogTable } from './merchant-catalog-table'
import { parsePoolRatio, poolTimestamp, positiveInteger } from './merchant-pool-model'
import { PoolButton as Button, PoolEmpty, PoolError, PoolField, PoolHeader, PoolLoading, PoolNavigation, PoolPager, PoolSelect } from './merchant-pool-ui'

export function MerchantPoolPage() {
  const { t, i18n } = useTranslation()
  const [params, setParams] = useSearchParams()
  const pools = useMerchantPools()
  const poolId = positiveInteger(params.get('pool') ?? '') ?? pools.data?.items[0]?.id
  const model = params.get('model') ?? ''
  const [search, setSearch] = useState('')
  const [status, setStatus] = useState('')
  const [sort, setSort] = useState('name')
  const [protocol, setProtocol] = useState('')
  const [streaming, setStreaming] = useState('')
  const [capability, setCapability] = useState('')
  const [minRatio, setMinRatio] = useState('')
  const [maxRatio, setMaxRatio] = useState('')
  const minimum = minRatio === '' ? undefined : parsePoolRatio(minRatio)
  const maximum = maxRatio === '' ? undefined : parsePoolRatio(maxRatio)
  const invalidPriceRange = (minRatio !== '' && minimum === undefined) || (maxRatio !== '' && maximum === undefined) || (minimum !== undefined && maximum !== undefined && minimum > maximum)
  const [page, setPage] = useState(0)
  const [windowSeconds, setWindowSeconds] = useState('86400')
  const [compared, setCompared] = useState<number[]>([])
  const [comparisonOpen, setComparisonOpen] = useState(false)
  const catalog = useMerchantCatalog(poolId, { model, search, status, sort: model ? sort : 'name', page: page + 1, protocol: protocol || undefined, capability: capability || undefined, min_ratio_micros: invalidPriceRange ? undefined : minimum, max_ratio_micros: invalidPriceRange ? undefined : maximum, streaming: streaming === '' ? undefined : streaming === 'true', window_seconds: Number(windowSeconds) })
  const stats = useMerchantStatistics(poolId, model, Number(windowSeconds), { protocol: protocol || undefined, streaming: streaming === '' ? undefined : streaming === 'true' })
  const timeline = useMerchantTimeline(poolId, model, Number(windowSeconds), { protocol: protocol || undefined, streaming: streaming === '' ? undefined : streaming === 'true' })
  const items = catalog.data?.items ?? []
  const statByService = new Map(stats.data?.items.map((item) => [item.service_id, item]))
  const timelineByService = new Map(timeline.data?.items.map((item) => [item.service_id, item]))
  const selected = items.filter((item) => compared.includes(item.id))
  useEffect(() => { setPage(0); setCompared([]) }, [poolId, model, search, status, sort, protocol, capability, minRatio, maxRatio, streaming, windowSeconds])
  useEffect(() => setCompared([]), [page])
  const setParam = (name: string, value: string) => { const next = new URLSearchParams(params); if (value) next.set(name, value); else next.delete(name); setParams(next, { replace: true }) }
  const refresh = () => { void pools.refetch(); if (poolId !== undefined) void Promise.all([catalog.refetch(), stats.refetch(), timeline.refetch()]) }
  const refreshing = pools.isFetching || catalog.isFetching || stats.isFetching || timeline.isFetching
  return <div className="flex min-w-0 flex-col gap-3">
    <PoolHeader title={t('merchantPool.title')} description={t('merchantPool.subtitle')} refreshing={refreshing} onRefresh={refresh}><Link className="rounded-lg bg-primary px-3 py-2 text-xs text-primary-foreground" to={`/console/merchant-pool/routes${poolId ? `?pool=${poolId}` : ''}`}>{t('merchantPool.createRoute')}</Link></PoolHeader>
    <PoolNavigation />
    <div className="grid grid-cols-2 gap-2 sm:flex sm:flex-wrap sm:items-end sm:[&>label]:w-36"><PoolField label={t('merchantPool.pool')}><PoolSelect label={t('merchantPool.pool')} value={poolId ? String(poolId) : ''} allowEmpty={false} disabled={pools.isPending} onChange={(value) => setParam('pool', value)} options={(pools.data?.items ?? []).map((pool) => ({ value: String(pool.id), label: pool.name }))} /></PoolField><PoolField label={t('merchantPool.model')}><Input size="sm" aria-label={t('merchantPool.model')} placeholder={t('merchantPool.modelPlaceholder')} value={model} onValueChange={(value) => setParam('model', value)} /></PoolField><PoolField label={t('merchantPool.search')}><Input size="sm" aria-label={t('merchantPool.search')} value={search} onValueChange={setSearch} /></PoolField><PoolField label={t('merchantPool.statusLabel')}><PoolSelect label={t('merchantPool.statusLabel')} value={status} onChange={setStatus} options={['published', 'paused', 'maintenance', 'suspended'].map((value) => ({ value, label: t(`merchantPool.status.${value}`) }))} /></PoolField><PoolField label={t('merchantPool.sort')}><PoolSelect label={t('merchantPool.sort')} allowEmpty={false} value={model ? sort : 'name'} onChange={setSort} options={[{ value: 'name', label: t('merchantPool.sortName') }, ...(model ? [{ value: 'price', label: t('merchantPool.sortPrice') }, { value: 'success_rate', label: t('merchantPool.sortSuccess') }, { value: 'latency', label: t('merchantPool.sortLatency') }] : [])]} /></PoolField><PoolField label={t('merchantPool.statWindow')}><PoolSelect label={t('merchantPool.statWindow')} allowEmpty={false} value={windowSeconds} onChange={setWindowSeconds} options={['900', '3600', '86400', '604800'].map((value) => ({ value, label: t(`merchantPool.windows.${value}`) }))} /></PoolField></div>
    <details className="border-b border-[var(--hairline)] pb-2"><summary className="cursor-pointer text-xs font-medium">{t('merchantPool.moreFilters')}</summary><div className="mt-2 grid grid-cols-2 gap-2 sm:flex sm:flex-wrap sm:items-end sm:[&>label]:w-36"><PoolField label={t('merchantPool.responseMode')}><PoolSelect label={t('merchantPool.responseMode')} allowEmpty={false} value={streaming} onChange={setStreaming} options={[{ value: '', label: t('merchantPool.all') }, { value: 'true', label: t('merchantPool.streaming') }, { value: 'false', label: t('merchantPool.nonStreaming') }]} /></PoolField><PoolField label={t('merchantPool.protocol')}><PoolSelect label={t('merchantPool.protocol')} value={protocol} onChange={setProtocol} options={[...merchantProtocols]} /></PoolField><PoolField label={t('merchantPool.capability')}><PoolSelect label={t('merchantPool.capability')} value={capability} onChange={setCapability} options={['streaming', 'tools', 'vision'].map((value) => ({ value, label: t(`merchantPool.${value}`) }))} /></PoolField><PoolField label={t('merchantPool.minimumRatio')}><Input aria-label={t('merchantPool.minimumRatio')} size="sm" inputMode="decimal" value={minRatio} onValueChange={setMinRatio} /></PoolField><PoolField label={t('merchantPool.maximumRatio')}><Input aria-label={t('merchantPool.maximumRatio')} size="sm" inputMode="decimal" value={maxRatio} onValueChange={setMaxRatio} /></PoolField></div>{invalidPriceRange ? <p role="alert" className="mt-2 text-xs text-danger">{t('merchantPool.invalidForm')}</p> : null}</details>
    {pools.isError ? <PoolError onRetry={() => void pools.refetch()} /> : null}
    <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"><span>{t('merchantPool.catalogCount', { count: catalog.data?.total ?? 0 })}{stats.data ? ` · ${t('merchantPool.statsAsOf', { value: poolTimestamp(stats.data.as_of, i18n.language) })}` : ''}</span><Button size="sm" variant="flat" isDisabled={!model || selected.length < 2} onPress={() => setComparisonOpen(true)}>{t('merchantPool.compare', { count: selected.length })}</Button></div>
    {stats.isError || timeline.isError ? <PoolError message={t('merchantPool.statsError')} onRetry={() => void Promise.all([stats.refetch(), timeline.refetch()])} /> : null}
    {pools.isPending ? <PoolLoading /> : !poolId ? <PoolEmpty>{t('merchantPool.noAccessiblePool')}</PoolEmpty> : catalog.isPending ? <PoolLoading /> : catalog.isError ? <PoolError onRetry={() => void catalog.refetch()} /> : items.length === 0 ? <PoolEmpty /> : <MerchantCatalogTable services={items} model={model} statistics={statByService} timelines={timelineByService} compared={compared} onCompare={(serviceId) => setCompared((current) => current.includes(serviceId) ? current.filter((id) => id !== serviceId) : current.length < 4 ? [...current, serviceId] : current)} />}
    {catalog.data ? <PoolPager page={page} hasMore={(page + 1) * catalog.data.page_size < catalog.data.total} loading={catalog.isFetching} onChange={setPage} /> : null}
    <details className="text-xs leading-5 text-muted-foreground"><summary className="w-fit cursor-pointer">{t('merchantPool.statisticsAndBilling')}</summary><p className="mt-1">{t('merchantPool.statisticsHint')}</p><p>{t('merchantPool.meteringHint')}</p></details>
    <Modal isOpen={comparisonOpen} onOpenChange={setComparisonOpen} size="5xl" scrollBehavior="inside"><ModalContent className="min-w-0 bg-surface-1 text-foreground"><ModalHeader>{t('merchantPool.compareTitle', { model })}</ModalHeader><ModalBody className="pb-5"><div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">{selected.map((service) => { const offer = service.models.find((item) => item.model === model); return <article key={service.id} className="grid content-start gap-4 rounded-lg border border-[var(--hairline)] p-3"><div><h3 className="text-sm font-semibold">{service.merchant_name}</h3><p className="mt-1 text-xs text-muted-foreground">{service.name}</p></div>{offer ? <MerchantModelPrice offer={offer} /> : null}<MerchantHealth ttftApplicable={merchantTextProtocol(service.protocol) && Boolean(offer?.supports_streaming) && streaming !== 'false'} stats={statByService.get(service.id)} /><Link className="text-xs text-primary hover:underline" to={`/console/merchant-pool/routes?pool=${poolId}&merchant=${service.merchant_id}`}>{t('merchantPool.addToRoute')}</Link></article> })}</div></ModalBody></ModalContent></Modal>
  </div>
}
