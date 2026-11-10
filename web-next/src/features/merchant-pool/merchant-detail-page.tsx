import { useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useMerchantCatalogDetail, useMerchantStatistics, useMerchantTimeline } from './merchant-pool-api'
import { MerchantHealth, MerchantTimelineStrip } from './merchant-catalog-card'
import { MerchantModelOffersTable } from './merchant-secondary-tables'
import { MerchantPublicMaintenance } from './merchant-public-maintenance'
import { MerchantServiceWatchButton } from './merchant-service-watch-button'
import { positiveInteger, merchantTextProtocol } from './merchant-pool-model'
import { PoolEmpty, PoolError, PoolField, PoolHeader, PoolLoading, PoolNavigation, PoolSelect, PoolStatus } from './merchant-pool-ui'

export function MerchantDetailPage() {
  const { t } = useTranslation()
  const { id } = useParams()
  const [params] = useSearchParams()
  const poolId = positiveInteger(params.get('pool') ?? '')
  const serviceId = positiveInteger(params.get('service') ?? '')
  const merchantId = positiveInteger(id ?? '')
  const serviceQuery = useMerchantCatalogDetail(poolId, serviceId)
  const service = serviceQuery.data
  const [model, setModel] = useState('')
  const [streaming, setStreaming] = useState('')
  const [window, setWindow] = useState('86400')
  const selectedModel = model || service?.models[0]?.model || ''
  const stats = useMerchantStatistics(poolId, selectedModel, Number(window), { protocol: service?.protocol, streaming: streaming === '' ? undefined : streaming === 'true' })
  const timeline = useMerchantTimeline(poolId, selectedModel, Number(window), { protocol: service?.protocol, streaming: streaming === '' ? undefined : streaming === 'true' })
  return <div className="flex min-w-0 flex-col gap-3"><PoolHeader title={service?.merchant_name ?? t('merchantPool.details')} description={service?.description ?? t('merchantPool.detailHint')} refreshing={serviceQuery.isFetching || stats.isFetching} onRefresh={() => { if (poolId && serviceId) void Promise.all([serviceQuery.refetch(), stats.refetch(), timeline.refetch()]) }} /><PoolNavigation />{!poolId || !serviceId || !merchantId ? <PoolEmpty>{t('merchantPool.invalidDetail')}</PoolEmpty> : serviceQuery.isPending ? <PoolLoading /> : serviceQuery.isError ? <PoolError onRetry={() => void serviceQuery.refetch()} /> : !service || service.merchant_id !== merchantId ? <PoolEmpty /> : <><section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[var(--hairline)] p-4"><div className="flex items-center gap-3"><h3 className="text-sm font-semibold">{service.name}</h3><PoolStatus status={service.status} /></div><div className="flex flex-wrap items-center gap-2"><MerchantServiceWatchButton poolId={poolId} serviceId={service.id} /><Link className="text-xs text-primary hover:underline" to={`/console/merchant-pool/routes?pool=${poolId}&merchant=${service.merchant_id}`}>{t('merchantPool.onlyMerchantKey')}</Link></div></section><MerchantPublicMaintenance notices={service.maintenance ?? []} /><section className="grid min-w-0 gap-2"><p className="text-xs text-muted-foreground">{t('merchantPool.quoteUnit')}</p><MerchantModelOffersTable models={service.models} /></section><section className="border-t border-[var(--hairline)] pt-3"><h3 className="mb-3 text-sm font-semibold">{t('merchantPool.modelHealth')}</h3><div className="mb-3 grid grid-cols-2 gap-2 sm:flex sm:flex-wrap sm:[&>label]:w-40"><PoolField label={t('merchantPool.model')}><PoolSelect label={t('merchantPool.model')} value={selectedModel} allowEmpty={false} onChange={setModel} options={service.models.map((item) => ({ value: item.model, label: item.model }))} /></PoolField><PoolField label={t('merchantPool.responseMode')}><PoolSelect label={t('merchantPool.responseMode')} value={streaming} allowEmpty={false} onChange={setStreaming} options={[{ value: '', label: t('merchantPool.all') }, { value: 'true', label: t('merchantPool.streaming') }, { value: 'false', label: t('merchantPool.nonStreaming') }]} /></PoolField><PoolField label={t('merchantPool.statWindow')}><PoolSelect label={t('merchantPool.statWindow')} value={window} allowEmpty={false} onChange={setWindow} options={['900', '3600', '86400', '604800'].map((value) => ({ value, label: t(`merchantPool.windows.${value}`) }))} /></PoolField></div>{stats.isError ? <PoolError message={t('merchantPool.statsError')} onRetry={() => void stats.refetch()} /> : <div><MerchantHealth ttftApplicable={merchantTextProtocol(service.protocol) && streaming !== 'false' && Boolean(service.models.find((item) => item.model === selectedModel)?.supports_streaming)} stats={stats.data?.items.find((item) => item.service_id === service.id)} /><MerchantTimelineStrip timeline={timeline.data?.items.find((item) => item.service_id === service.id)} /></div>}<p className="mt-3 text-xs leading-5 text-muted-foreground">{t('merchantPool.statisticsHint')}</p><p className="mt-2 text-xs leading-5 text-muted-foreground">{t('merchantPool.meteringHint')}</p></section></>}</div>
}
