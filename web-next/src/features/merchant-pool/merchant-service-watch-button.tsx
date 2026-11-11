import { useState } from 'react'
import { Bell, BellOff, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useMerchantAction, useMerchantServiceWatches } from './merchant-pool-api'
import { PoolButton as Button } from './merchant-pool-ui'

export function MerchantServiceWatchButton({ poolId, serviceId }: { poolId: number; serviceId: number }) {
  const { t } = useTranslation()
  const watches = useMerchantServiceWatches()
  const action = useMerchantAction()
  const [requestError, setRequestError] = useState(false)
  const watched = watches.data?.items.some((item) => item.pool_id === poolId && item.service_id === serviceId) ?? false
  const busy = action.isPending

  const toggle = async () => {
    setRequestError(false)
    try {
      await action.mutateAsync({
        url: watched
          ? `/api/merchant-pools/${poolId}/services/${serviceId}/watch`
          : `/api/merchant-pools/${poolId}/services/${serviceId}/watch`,
        method: watched ? 'delete' : 'put',
      })
    } catch {
      setRequestError(true)
    }
  }

  const label = watched ? t('merchantPool.market.unwatch') : t('merchantPool.market.watch')
  return <span className="inline-flex flex-col items-start gap-0.5">
    <Button
      type="button"
      size="sm"
      variant={watched ? 'flat' : 'bordered'}
      aria-label={label}
      aria-pressed={watched}
      isDisabled={busy || watches.isPending || watches.isError}
      isLoading={busy}
      onPress={() => void toggle()}
      title={watches.isError ? t('merchantPool.market.watchUnavailable') : undefined}
    >
      {watched ? <BellOff className="size-3.5" aria-hidden="true" /> : <Bell className="size-3.5" aria-hidden="true" />}
      {label}
    </Button>
    {requestError || watches.isError ? <span role="status" className="inline-flex items-center gap-1 text-[10px] text-danger">{t('merchantPool.market.watchUnavailable')}{watches.isError ? <button type="button" className="text-primary hover:underline" onClick={() => void watches.refetch()}><RefreshCw className="mr-0.5 inline size-3" aria-hidden="true" />{t('merchantPool.market.retryProviders')}</button> : null}</span> : null}
  </span>
}
