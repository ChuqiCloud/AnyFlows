import { useState } from 'react'
import { Drawer, DrawerBody, DrawerContent, DrawerHeader, Input, Textarea } from '@heroui/react'
import { useTranslation } from 'react-i18next'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useAdminMerchantEntries, useAdminMerchantFinance, useMerchantAction } from './merchant-pool-api'
import { clearMerchantFinancialAttempt, merchantFinancialAttempt } from './merchant-financial-attempt'
import { MerchantSettlementReversal } from './merchant-commission-settings'
import { merchantWithdrawalAmount, poolTimestamp } from './merchant-pool-model'
import { PoolButton as Button, PoolEmpty, PoolError, PoolField, PoolLoading, PoolSelect } from './merchant-pool-ui'

export function MerchantAdminFinance({ merchantId, onClose }: { merchantId: number; onClose: () => void }) {
  const { t, i18n } = useTranslation()
  const { policy, formatQuota } = useBalanceDisplay()
  const summary = useAdminMerchantFinance(merchantId)
  const [cursor, setCursor] = useState<string>()
  const [previous, setPrevious] = useState<Array<string | undefined>>([])
  const entries = useAdminMerchantEntries(merchantId, cursor)
  const [mode, setMode] = useState('freeze')
  const [amount, setAmount] = useState('')
  const [reason, setReason] = useState('')
  const [invalid, setInvalid] = useState(false)
  const [reversal, setReversal] = useState<string>()
  const action = useMerchantAction()
  const quota = merchantWithdrawalAmount(amount, policy.mode === 'quota' ? '1' : policy.quota_units_per_display_unit)
  const hold = async () => {
    if (!quota || !reason.trim() || new TextEncoder().encode(reason.trim()).length > 200 || !summary.data || quota > (mode === 'freeze' ? summary.data.available_quota : summary.data.frozen_quota)) { setInvalid(true); return }
    const key = `anyflows.merchant-hold.${merchantId}`
    const eventKey = merchantFinancialAttempt(key, JSON.stringify({ quota, mode, reason: reason.trim() }))
    try { await action.mutateAsync({ url: `/api/admin/merchant-finance/${merchantId}/hold`, body: { event_key: eventKey, amount_quota: quota, freeze: mode === 'freeze', reason: reason.trim() } }); clearMerchantFinancialAttempt(key); setAmount(''); setReason(''); setInvalid(false) } catch { /* Preserve a stable financial operation key for retry. */ }
  }
  return <Drawer isOpen size="3xl" placement="right" scrollBehavior="inside" onOpenChange={(open) => { if (!open && !action.isPending) onClose() }}><DrawerContent className="min-w-0 bg-surface-1 text-foreground"><DrawerHeader>{t('merchantPool.adminFinance', { id: merchantId })}</DrawerHeader><DrawerBody className="gap-4 pb-5">{summary.isPending ? <PoolLoading /> : summary.isError || !summary.data ? <PoolError onRetry={() => void summary.refetch()} /> : <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">{(['pending_quota', 'available_quota', 'withdrawing_quota', 'frozen_quota', 'debt_quota', 'total_gross_quota', 'total_commission_quota', 'total_paid_quota'] as const).map((field) => <article key={field} className="rounded-lg border border-[var(--hairline)] p-3"><p className="text-xs text-muted-foreground">{t(`merchantPool.financeLabels.${field}`)}</p><p className="mt-2 break-all text-sm font-semibold tabular-nums">{formatQuota(summary.data[field])}</p></article>)}</div>}
    <section className="grid gap-3 rounded-lg border border-[var(--hairline)] p-3"><h3 className="text-sm font-semibold">{t('merchantPool.fundsHold')}</h3><div className="grid gap-3 sm:grid-cols-2"><PoolField label={t('merchantPool.decision')}><PoolSelect label={t('merchantPool.decision')} allowEmpty={false} value={mode} onChange={setMode} options={[{ value: 'freeze', label: t('merchantPool.freeze') }, { value: 'unfreeze', label: t('merchantPool.unfreeze') }]} /></PoolField><PoolField label={policy.mode === 'custom_unit' ? t('merchantPool.withdrawalAmountUnit', { unit: policy.unit_name }) : t('merchantPool.withdrawalAmount')}><Input size="sm" aria-label={t('merchantPool.withdrawalAmount')} inputMode="decimal" value={amount} onValueChange={setAmount} /></PoolField></div><PoolField label={t('merchantPool.reviewReason')}><Textarea size="sm" aria-label={t('merchantPool.reviewReason')} value={reason} onValueChange={setReason} /></PoolField>{quota ? <p className="text-xs text-muted-foreground">{formatQuota(quota)}</p> : null}{invalid ? <PoolError message={t('merchantPool.invalidHold')} /> : null}{action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}<Button size="sm" color="primary" className="justify-self-end" isLoading={action.isPending} onPress={() => void hold()}>{t('merchantPool.confirm')}</Button></section>
    <h3 className="text-sm font-semibold">{t('merchantPool.earningsEntries')}</h3>{entries.isPending ? <PoolLoading /> : entries.isError ? <PoolError onRetry={() => void entries.refetch()} /> : entries.data.items.length === 0 ? <PoolEmpty /> : <div className="grid gap-2">{entries.data.items.map((entry) => <article key={entry.id} className="grid gap-2 rounded-lg border border-[var(--hairline)] p-3 text-xs"><div className="flex flex-wrap justify-between gap-2"><span className="font-medium">{t(`merchantPool.entryKinds.${entry.kind}`, { defaultValue: entry.kind })}</span><span className="text-muted-foreground">{poolTimestamp(entry.created_at, i18n.language)}</span></div><p className="tabular-nums">{t('merchantPool.gross')}: {formatQuota(entry.gross_quota)} · {t('merchantPool.commission')}: {formatQuota(entry.commission_quota)} · {t('merchantPool.net')}: {formatQuota(entry.net_quota)}</p><p className="break-all font-mono text-[0.6875rem] text-muted-foreground">{entry.reference}</p>{entry.kind === 'income_pending' && /^[0-9a-f]{32}$/.test(entry.reference) ? <Button size="sm" variant="light" color="danger" className="justify-self-end" onPress={() => setReversal(entry.reference)}>{t('merchantPool.reverseSettlement')}</Button> : null}</article>)}</div>}<div className="flex justify-end gap-2"><Button size="sm" variant="flat" isDisabled={previous.length === 0 || entries.isFetching} onPress={() => { setCursor(previous.at(-1)); setPrevious((current) => current.slice(0, -1)) }}>{t('merchantPool.previous')}</Button><Button size="sm" variant="flat" isDisabled={!entries.data?.next_cursor || entries.isFetching} onPress={() => { setPrevious((current) => [...current, cursor]); setCursor(entries.data?.next_cursor ?? undefined) }}>{t('merchantPool.next')}</Button></div>{reversal ? <MerchantSettlementReversal initialReference={reversal} onClose={() => setReversal(undefined)} /> : null}</DrawerBody></DrawerContent></Drawer>
}
