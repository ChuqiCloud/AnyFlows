import { useEffect, useState } from 'react'
import { Button, Input, Modal, ModalBody, ModalContent, ModalHeader } from '@heroui/react'
import { Link, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useMerchantCatalog, useMerchantPools, useMerchantStatistics } from './merchant-pool-api'
import { MerchantCatalogCard, MerchantHealth, MerchantModelPrice } from './merchant-catalog-card'
import { poolTimestamp, positiveInteger } from './merchant-pool-model'
import { PoolEmpty, PoolError, PoolField, PoolHeader, PoolLoading, PoolNavigation, PoolPager, PoolSelect } from './merchant-pool-ui'

export function MerchantPoolPage() {
  const { t, i18n } = useTranslation()
  const [params, setParams] = useSearchParams()
  const pools = useMerchantPools()
  const poolId = positiveInteger(params.get('pool') ?? '') ?? pools.data?.items[0]?.id
  const model = params.get('model') ?? ''
  const [search, setSearch] = useState('')
  const [status, setStatus] = useState('')
  const [sort, setSort] = useState('name')
  const [page, setPage] = useState(0)
  const [windowSeconds, setWindowSeconds] = useState('86400')
  const [compared, setCompared] = useState<number[]>([])
  const [comparisonOpen, setComparisonOpen] = useState(false)
  const catalog = useMerchantCatalog(poolId, { model, search, status, sort, page: page + 1 })
  const stats = useMerchantStatistics(poolId, model, Number(windowSeconds))
  const items = catalog.data?.items ?? []
  const statByService = new Map(stats.data?.items.map((item) => [item.service_id, item]))
  const selected = items.filter((item) => compared.includes(item.id))
  useEffect(() => { setPage(0); setCompared([]) }, [poolId, model, search, status, sort])
  useEffect(() => setCompared([]), [page])
  const setParam = (name: string, value: string) => { const next = new URLSearchParams(params); if (value) next.set(name, value); else next.delete(name); setParams(next, { replace: true }) }
  const refresh = () => { void pools.refetch(); if (poolId !== undefined) void Promise.all([catalog.refetch(), stats.refetch()]) }
  const refreshing = pools.isFetching || catalog.isFetching || stats.isFetching
  return <div className="flex flex-col gap-4">
    <PoolHeader title={t('merchantPool.title')} description={t('merchantPool.subtitle')} refreshing={refreshing} onRefresh={refresh}><Link className="rounded-lg bg-primary px-3 py-2 text-xs text-primary-foreground" to={`/console/merchant-pool/routes${poolId ? `?pool=${poolId}` : ''}`}>{t('merchantPool.createRoute')}</Link></PoolHeader>
    <PoolNavigation />
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-6"><PoolField label={t('merchantPool.pool')}><PoolSelect label={t('merchantPool.pool')} value={poolId ? String(poolId) : ''} allowEmpty={false} disabled={pools.isPending} onChange={(value) => setParam('pool', value)} options={(pools.data?.items ?? []).map((pool) => ({ value: String(pool.id), label: pool.name }))} /></PoolField><PoolField label={t('merchantPool.model')}><Input size="sm" aria-label={t('merchantPool.model')} placeholder={t('merchantPool.modelPlaceholder')} value={model} onValueChange={(value) => setParam('model', value)} /></PoolField><PoolField label={t('merchantPool.search')}><Input size="sm" aria-label={t('merchantPool.search')} value={search} onValueChange={setSearch} /></PoolField><PoolField label={t('merchantPool.statusLabel')}><PoolSelect label={t('merchantPool.statusLabel')} value={status} onChange={setStatus} options={['published', 'paused', 'suspended'].map((value) => ({ value, label: t(`merchantPool.status.${value}`) }))} /></PoolField><PoolField label={t('merchantPool.sort')}><PoolSelect label={t('merchantPool.sort')} allowEmpty={false} value={sort} onChange={setSort} options={[{ value: 'name', label: t('merchantPool.sortName') }, { value: 'price', label: t('merchantPool.sortPrice') }]} /></PoolField><PoolField label={t('merchantPool.statWindow')}><PoolSelect label={t('merchantPool.statWindow')} allowEmpty={false} value={windowSeconds} onChange={setWindowSeconds} options={['900', '3600', '86400', '604800'].map((value) => ({ value, label: t(`merchantPool.windows.${value}`) }))} /></PoolField></div>
    {pools.isError ? <PoolError onRetry={() => void pools.refetch()} /> : null}
    <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground"><span>{t('merchantPool.catalogCount', { count: catalog.data?.total ?? 0 })}{stats.data ? ` · ${t('merchantPool.statsAsOf', { value: poolTimestamp(stats.data.as_of, i18n.language) })}` : ''}</span><Button size="sm" variant="flat" isDisabled={!model || selected.length < 2} onPress={() => setComparisonOpen(true)}>{t('merchantPool.compare', { count: selected.length })}</Button></div>
    {stats.isError ? <PoolError message={t('merchantPool.statsError')} onRetry={() => void stats.refetch()} /> : null}
    {pools.isPending ? <PoolLoading /> : !poolId ? <PoolEmpty>{t('merchantPool.noAccessiblePool')}</PoolEmpty> : catalog.isPending ? <PoolLoading /> : catalog.isError ? <PoolError onRetry={() => void catalog.refetch()} /> : items.length === 0 ? <PoolEmpty /> : <div className="grid gap-3">{items.map((service) => <MerchantCatalogCard key={service.id} service={service} model={model} stats={statByService.get(service.id)} selected={compared.includes(service.id)} disabled={compared.length >= 4} onSelect={() => setCompared((current) => current.includes(service.id) ? current.filter((id) => id !== service.id) : current.length < 4 ? [...current, service.id] : current)} />)}</div>}
    {catalog.data ? <PoolPager page={page} hasMore={(page + 1) * catalog.data.page_size < catalog.data.total} loading={catalog.isFetching} onChange={setPage} /> : null}
    <p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.statisticsHint')}</p>
    <Modal isOpen={comparisonOpen} onOpenChange={setComparisonOpen} size="5xl" scrollBehavior="inside"><ModalContent><ModalHeader>{t('merchantPool.compareTitle', { model })}</ModalHeader><ModalBody className="pb-5"><div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">{selected.map((service) => { const offer = service.models.find((item) => item.model === model); return <article key={service.id} className="grid content-start gap-4 rounded-lg border border-[var(--hairline)] p-3"><div><h3 className="text-sm font-semibold">{service.merchant_name}</h3><p className="mt-1 text-xs text-muted-foreground">{service.name}</p></div>{offer ? <MerchantModelPrice offer={offer} /> : null}<MerchantHealth stats={statByService.get(service.id)} /><Link className="text-xs text-primary hover:underline" to={`/console/merchant-pool/routes?pool=${poolId}&merchant=${service.merchant_id}`}>{t('merchantPool.addToRoute')}</Link></article> })}</div></ModalBody></ModalContent></Modal>
  </div>
}
