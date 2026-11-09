import { useState } from 'react'
import { Modal, ModalBody, ModalContent, ModalFooter, ModalHeader } from '@heroui/react'
import { useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useMerchantAction, useMerchantPoolRoutes, useMerchantPools } from './merchant-pool-api'
import type { MerchantPoolRoute } from './merchant-pool-model'
import { positiveInteger } from './merchant-pool-model'
import { MerchantPoolRouteEditor } from './merchant-pool-route-editor'
import { MerchantRoutesTable } from './merchant-secondary-tables'
import { PoolButton as Button, PoolEmpty, PoolError, PoolHeader, PoolLoading, PoolNavigation } from './merchant-pool-ui'

export function MerchantPoolRoutesPage() {
  const { t } = useTranslation()
  const [params] = useSearchParams()
  const initialPoolId = positiveInteger(params.get('pool') ?? '')
  const initialMerchantId = positiveInteger(params.get('merchant') ?? '')
  const [editor, setEditor] = useState<MerchantPoolRoute | 'create' | undefined>(initialMerchantId ? 'create' : undefined)
  const [deleting, setDeleting] = useState<MerchantPoolRoute>()
  const routes = useMerchantPoolRoutes()
  const pools = useMerchantPools()
  const action = useMerchantAction()
  const remove = async () => { if (!deleting) return; try { await action.mutateAsync({ url: `/api/merchant-pool/routes/${deleting.id}`, method: 'delete' }); setDeleting(undefined) } catch { /* Retain confirmation on failure. */ } }
  return <div className="flex min-w-0 flex-col gap-3"><PoolHeader title={t('merchantPool.routes')} description={t('merchantPool.routeSubtitle')} refreshing={routes.isFetching || pools.isFetching} onRefresh={() => void Promise.all([routes.refetch(), pools.refetch()])}><Button color="primary" size="sm" isDisabled={!routes.isSuccess || !pools.isSuccess || pools.data.items.length === 0 || routes.data.items.length >= routes.data.capacity} onPress={() => setEditor('create')}>{t('merchantPool.createRoute')}</Button></PoolHeader><PoolNavigation />{routes.isPending || pools.isPending ? <PoolLoading /> : routes.isError || pools.isError ? <PoolError onRetry={() => void Promise.all([routes.refetch(), pools.refetch()])} /> : routes.data.items.length === 0 ? <PoolEmpty>{t('merchantPool.routesEmpty')}</PoolEmpty> : <MerchantRoutesTable routes={routes.data.items} pools={pools.data.items} onEdit={setEditor} onDelete={(route) => { action.reset(); setDeleting(route) }} />}{editor !== undefined && pools.data ? <MerchantPoolRouteEditor key={editor === 'create' ? `new-${initialMerchantId ?? ''}` : editor.id} route={editor === 'create' ? undefined : editor} pools={pools.data.items} initialPoolId={initialPoolId} initialMerchantId={initialMerchantId} onClose={() => setEditor(undefined)} /> : null}<Modal isOpen={Boolean(deleting)} onOpenChange={(open) => { if (!open && !action.isPending) setDeleting(undefined) }}><ModalContent className="min-w-0 bg-surface-1 text-foreground"><ModalHeader>{t('merchantPool.deleteRoute')}</ModalHeader><ModalBody><p className="text-sm">{t('merchantPool.deleteRouteHint', { name: deleting?.name })}</p>{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}</ModalBody><ModalFooter><Button size="sm" variant="flat" isDisabled={action.isPending} onPress={() => setDeleting(undefined)}>{t('merchantPool.cancel')}</Button><Button size="sm" color="danger" isLoading={action.isPending} onPress={() => void remove()}>{t('merchantPool.delete')}</Button></ModalFooter></ModalContent></Modal></div>
}
