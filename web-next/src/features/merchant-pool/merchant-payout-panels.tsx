import { useState } from 'react'
import { Input } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useMerchantAction } from './merchant-pool-api'
import { useMerchantPayoutAccount, useMerchantPayouts } from './merchant-extension-api'
import type { MerchantPayoutAccount } from './merchant-extension-model'
import type { MerchantWithdrawal } from './merchant-pool-model'
import { poolTimestamp } from './merchant-pool-model'
import { MerchantManagementTable } from './merchant-management-tables'
import { PoolButton as Button, PoolEmpty, PoolError, PoolField, PoolLoading, PoolStatus } from './merchant-pool-ui'

const cell = 'border-b border-[var(--hairline)] px-2 py-2 align-middle whitespace-nowrap'
export function MerchantPayoutAccountPanel() {
  const query = useMerchantPayoutAccount()
  return query.isPending ? <PoolLoading /> : query.isError ? <PoolError onRetry={() => void query.refetch()} /> : <PayoutAccountForm key={query.data.version} account={query.data} />
}
function PayoutAccountForm({ account }: { account: MerchantPayoutAccount }) {
  const { t } = useTranslation()
  const [label, setLabel] = useState(account.label)
  const [token, setToken] = useState('')
  const action = useMerchantAction()
  const [invalid, setInvalid] = useState(false)
  const save = async (clear: boolean) => {
    if (!clear && (!label.trim() || !token.trim())) { setInvalid(true); return }
    try { await action.mutateAsync({ url: '/api/merchant/payout-account', method: 'put', body: { expected_version: account.version, label: clear ? '' : label.trim(), clear, ...(!clear ? { account: token.trim() } : {}) } }); setToken(''); setInvalid(false); action.reset() } catch { /* Write-only account remains in component memory for retry. */ }
  }
  return <section className="grid gap-3 border-b border-[var(--hairline)] pb-3"><div className="flex items-center gap-2"><h4 className="text-xs font-semibold">{t('merchantPool.payoutAccount')}</h4><PoolStatus status={account.configured ? 'active' : 'disabled'} /></div><p className="text-xs leading-5 text-muted-foreground">{t('merchantPool.payoutAccountHint')}</p><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.payoutAccountLabel')}><Input aria-label={t('merchantPool.payoutAccountLabel')} size="sm" maxLength={80} value={label} onValueChange={setLabel} /></PoolField><PoolField label={t('merchantPool.payoutAccountToken')}><Input aria-label={t('merchantPool.payoutAccountToken')} size="sm" type="password" autoComplete="new-password" maxLength={1024} value={token} onValueChange={setToken} /></PoolField></div>{invalid ? <PoolError message={t('merchantPool.invalidForm')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}<div className="flex justify-end gap-2">{account.configured ? <Button size="sm" variant="light" color="danger" isDisabled={action.isPending} onPress={() => void save(true)}>{t('merchantPool.clearPayoutAccount')}</Button> : null}<Button size="sm" color="primary" isLoading={action.isPending} onPress={() => void save(false)}>{t('merchantPool.save')}</Button></div></section>
}

export function MerchantWithdrawalsTable({ withdrawals, formatQuota, admin = false, payoutEnabled = false, onReview }: { withdrawals: MerchantWithdrawal[]; formatQuota: (value: number) => string; admin?: boolean; payoutEnabled?: boolean; onReview?: (withdrawal: MerchantWithdrawal) => void }) {
  const { t, i18n } = useTranslation()
  const action = useMerchantAction()
  const execute = async (id: number, operation: 'payout' | 'reconcile') => { try { await action.mutateAsync({ url: `/api/admin/merchant-withdrawals/${id}/${operation}`, body: {} }) } catch { /* Retain current attempt state until confirmation. */ } }
  return <div className="grid min-w-0 gap-2">{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}<MerchantManagementTable label={t('merchantPool.withdrawals')} columns={['reference', ...(admin ? ['merchant'] : []), 'amount', 'status', 'payment', 'reason', 'updated', ...(admin ? ['actions'] : [])]}>{withdrawals.map((withdrawal) => <tr key={withdrawal.id}><td className={`${cell} font-mono`}>#{withdrawal.id}</td>{admin ? <td className={cell}>#{withdrawal.merchant_id}</td> : null}<td className={`${cell} font-medium`}>{formatQuota(withdrawal.amount_quota)}{withdrawal.payout ? <span className="mt-0.5 block text-[11px] text-muted-foreground">{(withdrawal.payout.amount_minor / 100).toFixed(2)} {withdrawal.payout.currency}</span> : null}</td><td className={cell}><PoolStatus status={withdrawal.payout?.status ?? withdrawal.status} /></td><td className={`${cell} font-mono`}><span className="block max-w-48 truncate" title={withdrawal.payout?.reference ?? withdrawal.reference ?? undefined}>{withdrawal.payout?.reference ?? withdrawal.reference ?? '—'}</span></td><td className={cell}><span className="block max-w-60 truncate" title={withdrawal.payout?.reason ?? withdrawal.reason ?? undefined}>{withdrawal.payout?.reason ? t(`merchantPool.payoutReasons.${withdrawal.payout.reason}`, { defaultValue: t('merchantPool.payoutUncertain') }) : withdrawal.reason || '—'}</span></td><td className={cell}>{poolTimestamp(withdrawal.payout?.updated_at ?? withdrawal.updated_at, i18n.language)}</td>{admin ? <td className={cell}>{withdrawal.payout ? ['unknown', 'processing'].includes(withdrawal.payout.status) ? <Button size="sm" variant="light" isLoading={action.isPending} onPress={() => void execute(withdrawal.id, 'reconcile')}>{t('merchantPool.reconcilePayout')}</Button> : '—' : withdrawal.status === 'pending' ? <div className="flex gap-1"><Button size="sm" variant="light" isDisabled={action.isPending} onPress={() => onReview?.(withdrawal)}>{t('merchantPool.reviewWithdrawal')}</Button><Button size="sm" variant="flat" title={!payoutEnabled ? t('merchantPool.payoutDisabled') : undefined} isDisabled={!payoutEnabled} isLoading={action.isPending} onPress={() => void execute(withdrawal.id, 'payout')}>{t('merchantPool.queuePayout')}</Button></div> : '—'}</td> : null}</tr>)}</MerchantManagementTable></div>
}

export function MerchantPayoutsPanel({ admin = false }: { admin?: boolean }) {
  const { t, i18n } = useTranslation()
  const [cursor, setCursor] = useState<string>()
  const [previous, setPrevious] = useState<Array<string | undefined>>([])
  const query = useMerchantPayouts(admin, cursor)
  const action = useMerchantAction()
  const reconcile = async (id: number) => { try { await action.mutateAsync({ url: `/api/admin/merchant-withdrawals/${id}/reconcile`, body: {} }) } catch { /* A query never repeats transfer submission. */ } }
  return <section className="grid min-w-0 gap-3"><h4 className="text-xs font-semibold">{t('merchantPool.adminTabs.payouts')}</h4><p className="text-xs text-muted-foreground">{t('merchantPool.payoutStatusHint')}</p>{query.isPending ? <PoolLoading /> : query.isError ? <PoolError onRetry={() => void query.refetch()} /> : query.data.items.length === 0 ? <PoolEmpty /> : <MerchantManagementTable label={t('merchantPool.adminTabs.payouts')} columns={['reference', ...(admin ? ['merchant'] : []), 'amount', 'status', 'payment', 'reason', 'updated', ...(admin ? ['actions'] : [])]}>{query.data.items.map((payout) => <tr key={payout.withdrawal_id}><td className={`${cell} font-mono`}>#{payout.withdrawal_id}</td>{admin ? <td className={cell}>#{payout.merchant_id}</td> : null}<td className={cell}>{(payout.amount_minor / 100).toFixed(2)} {payout.currency}</td><td className={cell}><PoolStatus status={payout.status} /></td><td className={`${cell} font-mono`}>{payout.reference || '—'}</td><td className={cell}>{payout.reason ? t(`merchantPool.payoutReasons.${payout.reason}`, { defaultValue: t('merchantPool.payoutUncertain') }) : '—'}</td><td className={cell}>{poolTimestamp(payout.updated_at, i18n.language)}</td>{admin ? <td className={cell}>{['processing', 'unknown'].includes(payout.status) ? <Button size="sm" variant="light" isLoading={action.isPending} onPress={() => void reconcile(payout.withdrawal_id)}>{t('merchantPool.reconcilePayout')}</Button> : '—'}</td> : null}</tr>)}</MerchantManagementTable>}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}<div className="flex justify-end gap-2"><Button size="sm" variant="flat" isDisabled={previous.length === 0 || query.isFetching} onPress={() => { setCursor(previous.at(-1)); setPrevious((current) => current.slice(0, -1)) }}>{t('merchantPool.previous')}</Button><Button size="sm" variant="flat" isDisabled={!query.data?.next_cursor || query.isFetching} onPress={() => { setPrevious((current) => [...current, cursor]); setCursor(query.data?.next_cursor ?? undefined) }}>{t('merchantPool.next')}</Button></div></section>
}
