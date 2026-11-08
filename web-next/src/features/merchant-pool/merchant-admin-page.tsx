import { useState } from 'react'
import { Button } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useMerchantAction, useMerchantPools, useMerchantProfiles, useMerchantServices, useMerchantWithdrawals } from './merchant-pool-api'
import { MerchantAdminReview, type MerchantReviewTarget } from './merchant-admin-review'
import { MerchantAdminFinance } from './merchant-admin-finance'
import { MerchantPoolAdminEditor } from './merchant-pool-admin-editor'
import { MerchantCommissionSettings, MerchantSettlementReversal, MerchantTermsEditor } from './merchant-commission-settings'
import type { MerchantPool } from './merchant-pool-model'
import { MerchantAdminPoolsTable, MerchantAdminProfilesTable, MerchantAdminServicesTable, MerchantAdminWithdrawalsTable } from './merchant-management-tables'
import { PoolEmpty, PoolError, PoolHeader, PoolLoading, PoolPager, PoolSelect } from './merchant-pool-ui'

export function MerchantAdminPage() {
  const { t } = useTranslation()
  const { formatQuota } = useBalanceDisplay()
  const [panel, setPanel] = useState<'pools' | 'merchants' | 'services' | 'withdrawals'>('pools')
  const [page, setPage] = useState(0)
  const [status, setStatus] = useState('')
  const pools = useMerchantPools(true, page + 1)
  const termsPools = useMerchantPools(true)
  const merchants = useMerchantProfiles(page + 1, status)
  const services = useMerchantServices(true, page + 1, status)
  const [withdrawalCursor, setWithdrawalCursor] = useState<string>()
  const [withdrawalPrevious, setWithdrawalPrevious] = useState<Array<string | undefined>>([])
  const withdrawals = useMerchantWithdrawals(true, status, true, withdrawalCursor)
  const [poolEditor, setPoolEditor] = useState<MerchantPool | 'create'>()
  const [review, setReview] = useState<MerchantReviewTarget>()
  const [termsMerchant, setTermsMerchant] = useState<number>()
  const [financeMerchant, setFinanceMerchant] = useState<number>()
  const [reversalOpen, setReversalOpen] = useState(false)
  const restore = useMerchantAction()
  const resumeService = async (id: number) => { try { await restore.mutateAsync({ url: `/api/admin/merchant-services/${id}/resume`, body: {} }) } catch { /* Keep suspension visible until the server confirms restoration. */ } }
  const query = panel === 'pools' ? pools : panel === 'merchants' ? merchants : panel === 'services' ? services : withdrawals
  const paged = panel === 'pools' ? pools.data : panel === 'merchants' ? merchants.data : panel === 'services' ? services.data : undefined
  return <div className="flex min-w-0 flex-col gap-3">
    <PoolHeader title={t('merchantPool.adminTitle')} description={t('merchantPool.adminSubtitle')} refreshing={query.isFetching} onRefresh={() => void query.refetch()}>
      {panel === 'pools' ? <Button color="primary" size="sm" onPress={() => setPoolEditor('create')}>{t('merchantPool.createPool')}</Button> : null}
      <Button size="sm" variant="bordered" onPress={() => setReversalOpen(true)}>{t('merchantPool.reverseSettlement')}</Button>
    </PoolHeader>
    {panel === 'pools' ? <details className="border-b border-[var(--hairline)] pb-2"><summary className="w-fit cursor-pointer text-xs font-medium">{t('merchantPool.globalCommission')}</summary><div className="mt-2"><MerchantCommissionSettings /></div></details> : null}
    <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--hairline)] pb-2">
      <div role="group" aria-label={t('merchantPool.adminTitle')} className="flex flex-wrap gap-1">{(['pools', 'merchants', 'services', 'withdrawals'] as const).map((value) => <Button key={value} size="sm" variant={panel === value ? 'flat' : 'light'} onPress={() => { setPanel(value); setPage(0); setStatus(''); setWithdrawalCursor(undefined); setWithdrawalPrevious([]) }}>{t(`merchantPool.adminTabs.${value}`)}</Button>)}</div>
      {panel !== 'pools' ? <div className="w-40"><PoolSelect label={t('merchantPool.statusLabel')} value={status} onChange={(value) => { setStatus(value); setPage(0); setWithdrawalCursor(undefined); setWithdrawalPrevious([]) }} options={(panel === 'merchants' ? ['pending', 'approved', 'rejected', 'suspended'] : panel === 'services' ? ['draft', 'pending', 'published', 'paused', 'suspended', 'rejected'] : ['pending', 'paid', 'rejected']).map((value) => ({ value, label: t(`merchantPool.status.${value}`) }))} /></div> : null}
    </div>
    {restore.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}
    {query.isPending ? <PoolLoading /> : query.isError ? <PoolError onRetry={() => void query.refetch()} /> : <>
      {panel === 'pools' ? pools.data?.items.length ? <MerchantAdminPoolsTable pools={pools.data.items} onEdit={setPoolEditor} /> : <PoolEmpty /> : null}
      {panel === 'merchants' ? merchants.data?.items.length ? <MerchantAdminProfilesTable merchants={merchants.data.items} onFinance={setFinanceMerchant} onTerms={setTermsMerchant} onReview={(merchant) => setReview({ kind: 'merchant', item: merchant })} /> : <PoolEmpty /> : null}
      {panel === 'services' ? services.data?.items.length ? <MerchantAdminServicesTable services={services.data.items} restoring={restore.isPending} onResume={(id) => void resumeService(id)} onReview={(service) => setReview({ kind: 'service', item: service })} /> : <PoolEmpty /> : null}
      {panel === 'withdrawals' ? withdrawals.data?.items.length ? <MerchantAdminWithdrawalsTable withdrawals={withdrawals.data.items} formatQuota={formatQuota} onReview={(withdrawal) => setReview({ kind: 'withdrawal', item: withdrawal })} /> : <PoolEmpty /> : null}
      {panel === 'withdrawals' ? <div className="flex justify-end gap-2"><Button size="sm" variant="flat" isDisabled={withdrawalPrevious.length === 0 || withdrawals.isFetching} onPress={() => { setWithdrawalCursor(withdrawalPrevious.at(-1)); setWithdrawalPrevious((current) => current.slice(0, -1)) }}>{t('merchantPool.previous')}</Button><Button size="sm" variant="flat" isDisabled={!withdrawals.data?.next_cursor || withdrawals.isFetching} onPress={() => { setWithdrawalPrevious((current) => [...current, withdrawalCursor]); setWithdrawalCursor(withdrawals.data?.next_cursor ?? undefined) }}>{t('merchantPool.next')}</Button></div> : null}
      {paged ? <PoolPager page={page} hasMore={(page + 1) * paged.page_size < paged.total} loading={query.isFetching} onChange={setPage} /> : null}
    </>}
    {poolEditor !== undefined ? <MerchantPoolAdminEditor key={poolEditor === 'create' ? 'new-pool' : poolEditor.id} pool={poolEditor === 'create' ? undefined : poolEditor} onClose={() => setPoolEditor(undefined)} /> : null}
    {financeMerchant !== undefined ? <MerchantAdminFinance merchantId={financeMerchant} onClose={() => setFinanceMerchant(undefined)} /> : null}
    {termsMerchant !== undefined && termsPools.data ? <MerchantTermsEditor merchantId={termsMerchant} pools={termsPools.data.items} onClose={() => setTermsMerchant(undefined)} /> : null}
    {reversalOpen ? <MerchantSettlementReversal onClose={() => setReversalOpen(false)} /> : null}
    {review ? <MerchantAdminReview key={`${review.kind}-${review.item.id}`} target={review} onClose={() => setReview(undefined)} /> : null}
  </div>
}
