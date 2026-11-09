import { merchantProtocols } from './merchant-pool-model'
import { useState } from 'react'
import { Input } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useOwnedMerchantServiceDirectory, useOwnedMerchantStatistics } from './merchant-pool-api'
import { MerchantStatisticsTable } from './merchant-secondary-tables'
import { poolTimestamp } from './merchant-pool-model'
import { PoolEmpty, PoolError, PoolField, PoolHeader, PoolLoading, PoolSelect } from './merchant-pool-ui'

export function MerchantStatisticsPanel() {
  const { t, i18n } = useTranslation()
  const [model, setModel] = useState('')
  const [protocol, setProtocol] = useState('')
  const [streaming, setStreaming] = useState('')
  const [windowSeconds, setWindowSeconds] = useState('86400')
  const directory = useOwnedMerchantServiceDirectory()
  const query = useOwnedMerchantStatistics(model, Number(windowSeconds), { protocol: protocol || undefined, streaming: streaming === '' ? undefined : streaming === 'true' })
  const names = new Map(directory.data?.map((service) => [service.id, service]))
  return <section className="grid min-w-0 gap-3"><PoolHeader title={t('merchantPool.ownStatistics')} description={t('merchantPool.ownStatisticsHint')} refreshing={query.isFetching || directory.isFetching} onRefresh={() => void Promise.all([query.refetch(), directory.refetch()])} /><div className="grid grid-cols-2 gap-2 sm:flex sm:flex-wrap sm:[&>label]:w-40"><PoolField label={t('merchantPool.model')}><Input aria-label={t('merchantPool.model')} size="sm" value={model} onValueChange={setModel} placeholder={t('merchantPool.modelPlaceholder')} /></PoolField><PoolField label={t('merchantPool.protocol')}><PoolSelect label={t('merchantPool.protocol')} value={protocol} onChange={setProtocol} options={[...merchantProtocols]} /></PoolField><PoolField label={t('merchantPool.responseMode')}><PoolSelect label={t('merchantPool.responseMode')} allowEmpty={false} value={streaming} onChange={setStreaming} options={[{ value: '', label: t('merchantPool.all') }, { value: 'true', label: t('merchantPool.streaming') }, { value: 'false', label: t('merchantPool.nonStreaming') }]} /></PoolField><PoolField label={t('merchantPool.statWindow')}><PoolSelect label={t('merchantPool.statWindow')} allowEmpty={false} value={windowSeconds} onChange={setWindowSeconds} options={['900', '3600', '86400', '604800'].map((value) => ({ value, label: t(`merchantPool.windows.${value}`) }))} /></PoolField></div>{directory.isError ? <PoolError onRetry={() => void directory.refetch()} /> : null}{query.isPending ? <PoolLoading /> : query.isError ? <PoolError message={t('merchantPool.statsError')} onRetry={() => void query.refetch()} /> : query.data.items.length === 0 ? <PoolEmpty>{t('merchantPool.ownStatisticsEmpty')}</PoolEmpty> : <MerchantStatisticsTable statistics={query.data.items} services={names} />}{query.data ? <p className="text-xs text-muted-foreground">{t('merchantPool.statsAsOf', { value: poolTimestamp(query.data.as_of, i18n.language) })}</p> : null}<details className="text-xs leading-5 text-muted-foreground"><summary className="w-fit cursor-pointer">{t('merchantPool.statisticsAndBilling')}</summary><p>{t('merchantPool.statisticsHint')}</p></details></section>
}
