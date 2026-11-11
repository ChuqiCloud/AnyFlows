import { useState } from 'react'
import { Input } from '@heroui/react'
import { Download } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { merchantWithdrawalAmount, poolTimestamp, type MerchantWithdrawal } from './merchant-pool-model'
import { clearMerchantFinancialAttempt, merchantFinancialAttempt } from './merchant-financial-attempt'
import { downloadMerchantFinanceReport, useMerchantAction, useMerchantEntries, useMerchantFinance, useMerchantFinanceReport, useMerchantWithdrawals } from './merchant-pool-api'
import { MerchantPayoutAccountPanel, MerchantPayoutsPanel, MerchantWithdrawalsTable } from './merchant-payout-panels'
import { PoolButton as Button, PoolEmpty, PoolError, PoolField, PoolLoading } from './merchant-pool-ui'

function utcDate(value: Date) { return value.toISOString().slice(0, 10) }
function initialReportRange() {
  const end = new Date()
  const start = new Date(end)
  start.setUTCDate(start.getUTCDate() - 29)
  return { from: utcDate(start), to: utcDate(end) }
}

export function MerchantFinancePanel() {
  const { t, i18n } = useTranslation()
  const { formatQuota, policy } = useBalanceDisplay()
  const summary = useMerchantFinance()
  const [cursor, setCursor] = useState<string>()
  const [previousCursors, setPreviousCursors] = useState<Array<string | undefined>>([])
  const entries = useMerchantEntries(cursor)
  const [withdrawalCursor, setWithdrawalCursor] = useState<string>()
  const [withdrawalPrevious, setWithdrawalPrevious] = useState<Array<string | undefined>>([])
  const withdrawals = useMerchantWithdrawals(false, '', true, withdrawalCursor)
  const action = useMerchantAction<MerchantWithdrawal>()
  const [reportRange] = useState(initialReportRange)
  const [reportFrom, setReportFrom] = useState(reportRange.from)
  const [reportTo, setReportTo] = useState(reportRange.to)
  const [reportGranularity, setReportGranularity] = useState<'day' | 'month'>('day')
  const report = useMerchantFinanceReport(reportFrom, reportTo, reportGranularity)
  const [reportPage, setReportPage] = useState(0)
  const reportPageSize = 50
  const reportRows = report.data?.rows.slice(reportPage * reportPageSize, (reportPage + 1) * reportPageSize) ?? []
  const [reportDownloadPending, setReportDownloadPending] = useState(false)
  const [reportDownloadError, setReportDownloadError] = useState(false)
  const [amount, setAmount] = useState('')
  const [invalid, setInvalid] = useState(false)
  const quota = merchantWithdrawalAmount(amount, policy.mode === 'quota' ? '1' : policy.quota_units_per_display_unit)
  const withdraw = async () => {
    if (!summary.data || quota === undefined || quota > summary.data.available_quota) { setInvalid(true); return }
    const storageKey = `anyflows.merchant-withdrawal.${summary.data.merchant_id}`
    const eventKey = merchantFinancialAttempt(storageKey, String(quota))
    try { await action.mutateAsync({ url: '/api/merchant/withdrawals', body: { event_key: eventKey, amount_quota: quota } }); setAmount(''); setInvalid(false); clearMerchantFinancialAttempt(storageKey) } catch { /* Preserve the attempt key for a safe retry. */ }
  }
  const downloadReport = async () => {
    if (reportDownloadPending) return
    setReportDownloadPending(true); setReportDownloadError(false)
    try {
      const blob = await downloadMerchantFinanceReport(reportFrom, reportTo, reportGranularity)
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a'); link.href = url; link.download = `merchant-finance-${reportFrom}-${reportTo}-${reportGranularity}.csv`; link.click()
      window.setTimeout(() => URL.revokeObjectURL(url), 1_000)
    } catch { setReportDownloadError(true) } finally { setReportDownloadPending(false) }
  }
  return <section className="grid gap-4">
    <h3 className="text-sm font-semibold">{t('merchantPool.finance')}</h3>
    <MerchantPayoutAccountPanel />
    {summary.isPending ? <PoolLoading /> : summary.isError || !summary.data ? <PoolError onRetry={() => void summary.refetch()} /> : <>
      <div className="grid grid-cols-2 gap-3 xl:grid-cols-4">{(['pending_quota', 'available_quota', 'withdrawing_quota', 'frozen_quota', 'debt_quota', 'total_gross_quota', 'total_commission_quota', 'total_paid_quota'] as const).map((field) => <article key={field} className="rounded-lg border border-[var(--hairline)] p-3"><p className="text-xs text-muted-foreground">{t(`merchantPool.financeLabels.${field}`)}</p><p className="mt-2 break-all text-base font-semibold tabular-nums">{formatQuota(summary.data[field])}</p></article>)}</div>
      <div className="grid items-end gap-3 rounded-lg border border-[var(--hairline)] p-4 sm:grid-cols-[minmax(0,1fr)_auto]"><PoolField label={policy.mode === 'custom_unit' ? t('merchantPool.withdrawalAmountUnit', { unit: policy.unit_name }) : t('merchantPool.withdrawalAmount')} hint={quota ? t('merchantPool.withdrawalPreview', { value: formatQuota(quota) }) : t('merchantPool.withdrawalHint')}><Input aria-label={t('merchantPool.withdrawalAmount')} size="sm" inputMode="decimal" value={amount} onValueChange={setAmount} isDisabled={action.isPending} /></PoolField><Button size="sm" color="primary" isLoading={action.isPending} isDisabled={summary.data.available_quota <= 0 || summary.data.frozen_quota > 0 || summary.data.debt_quota > 0} onPress={() => void withdraw()}>{t('merchantPool.requestWithdrawal')}</Button></div>
    </>}
    {invalid ? <PoolError message={t('merchantPool.invalidWithdrawal')} /> : null}
    {action.isError ? <PoolError message={t('merchantPool.saveError')} /> : null}
    <div className="grid gap-3"><h4 className="text-xs font-semibold">{t('merchantPool.withdrawals')}</h4>{withdrawals.isPending ? <PoolLoading /> : withdrawals.isError ? <PoolError onRetry={() => void withdrawals.refetch()} /> : withdrawals.data.items.length === 0 ? <PoolEmpty /> : <MerchantWithdrawalsTable withdrawals={withdrawals.data.items} formatQuota={formatQuota} />}<div className="flex justify-end gap-2"><Button size="sm" variant="flat" isDisabled={withdrawalPrevious.length === 0 || withdrawals.isFetching} onPress={() => { setWithdrawalCursor(withdrawalPrevious.at(-1)); setWithdrawalPrevious((current) => current.slice(0, -1)) }}>{t('merchantPool.previous')}</Button><Button size="sm" variant="flat" isDisabled={!withdrawals.data?.next_cursor || withdrawals.isFetching} onPress={() => { setWithdrawalPrevious((current) => [...current, withdrawalCursor]); setWithdrawalCursor(withdrawals.data?.next_cursor ?? undefined) }}>{t('merchantPool.next')}</Button></div></div>
    <MerchantPayoutsPanel />
    <section aria-label={t('merchantPool.financeReport.title')} className="grid min-w-0 gap-3 border-t border-[var(--hairline)] pt-4">
      <div className="flex flex-wrap items-end justify-between gap-3"><div><h4 className="text-xs font-semibold">{t('merchantPool.financeReport.title')}</h4><p className="mt-1 text-[11px] text-muted-foreground">{t('merchantPool.financeReport.utcHint')}</p></div><div className="flex flex-wrap items-end gap-2"><PoolField label={t('merchantPool.financeReport.from')}><Input aria-label={t('merchantPool.financeReport.from')} type="date" size="sm" value={reportFrom} onValueChange={(value) => { setReportFrom(value); setReportPage(0) }} /></PoolField><PoolField label={t('merchantPool.financeReport.to')}><Input aria-label={t('merchantPool.financeReport.to')} type="date" size="sm" value={reportTo} onValueChange={(value) => { setReportTo(value); setReportPage(0) }} /></PoolField><PoolField label={t('merchantPool.financeReport.granularity')}><select aria-label={t('merchantPool.financeReport.granularity')} className="h-9 rounded-md border border-[var(--hairline)] bg-background px-2 text-xs" value={reportGranularity} onChange={(event) => { setReportGranularity(event.target.value as 'day' | 'month'); setReportPage(0) }}><option value="day">{t('merchantPool.financeReport.daily')}</option><option value="month">{t('merchantPool.financeReport.monthly')}</option></select></PoolField><Button size="sm" variant="bordered" isDisabled={reportDownloadPending || report.isPending || !report.data} onPress={() => void downloadReport()}><Download className="size-3.5" aria-hidden="true" />{t('merchantPool.financeReport.export')}</Button></div></div>
      {reportDownloadError ? <PoolError message={t('merchantPool.financeReport.exportError')} /> : null}
      {!reportFrom || !reportTo ? <p role="alert" className="text-xs text-danger">{t('merchantPool.financeReport.dateRequired')}</p> : report.isPending ? <PoolLoading /> : report.isError ? <PoolError onRetry={() => void report.refetch()} /> : report.data ? <>
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 xl:grid-cols-6">{(['sales_gross_quota', 'commission_quota', 'merchant_net_quota', 'refunded_gross_quota', 'refunded_commission_quota', 'refunded_net_quota'] as const).map((field) => <article key={field} className="rounded-md border border-[var(--hairline)] p-2.5"><p className="text-[10px] text-muted-foreground">{t(`merchantPool.financeReport.totals.${field}`)}</p><p className="mt-1 break-all text-xs font-semibold tabular-nums">{formatQuota(report.data.totals[field])}</p></article>)}</div>
        {report.data.rows.length === 0 ? <PoolEmpty /> : <><div className="overflow-x-auto rounded-lg border border-[var(--hairline)]"><table className="w-full min-w-[1040px] text-left text-xs"><thead className="bg-surface-2 text-muted-foreground"><tr>{['period', 'service', 'model', 'gross_quota', 'commission_quota', 'net_quota', 'refunded_net_quota'].map((field) => <th key={field} className="whitespace-nowrap p-2.5 font-medium">{t(`merchantPool.financeReport.columns.${field}`)}</th>)}</tr></thead><tbody>{reportRows.map((row, index) => <tr key={`${row.period}-${row.service_id}-${row.model}-${index}`} className="border-t border-[var(--hairline)]"><td className="whitespace-nowrap p-2.5">{row.period}</td><td className="max-w-52 truncate p-2.5" title={row.service_name}>{row.service_name}</td><td className="max-w-56 truncate p-2.5 font-mono" title={row.model}>{row.model}</td><td className="p-2.5 tabular-nums">{formatQuota(row.gross_quota)}</td><td className="p-2.5 tabular-nums">{formatQuota(row.commission_quota)}</td><td className="p-2.5 tabular-nums">{formatQuota(row.net_quota)}</td><td className="p-2.5 tabular-nums">{formatQuota(row.refunded_net_quota)}</td></tr>)}</tbody></table></div><div className="flex items-center justify-end gap-2"><Button size="sm" variant="flat" isDisabled={reportPage === 0} onPress={() => setReportPage((page) => page - 1)}>{t('merchantPool.previous')}</Button><span className="text-xs text-muted-foreground">{t('merchantPool.financeReport.page', { page: reportPage + 1, pages: Math.ceil(report.data.rows.length / reportPageSize) })}</span><Button size="sm" variant="flat" isDisabled={(reportPage + 1) * reportPageSize >= report.data.rows.length} onPress={() => setReportPage((page) => page + 1)}>{t('merchantPool.next')}</Button></div></>}
      </> : null}
    </section>
    <div className="grid gap-3"><h4 className="text-xs font-semibold">{t('merchantPool.earningsEntries')}</h4>{entries.isPending ? <PoolLoading /> : entries.isError ? <PoolError onRetry={() => void entries.refetch()} /> : entries.data.items.length === 0 ? <PoolEmpty /> : <div className="overflow-x-auto rounded-lg border border-[var(--hairline)]"><table className="w-full text-left text-xs"><thead className="bg-surface-2 text-muted-foreground"><tr>{['entryType', 'gross', 'commission', 'net', 'createdAt'].map((field) => <th key={field} className="whitespace-nowrap p-3 font-medium">{t(`merchantPool.${field}`)}</th>)}</tr></thead><tbody>{entries.data.items.map((entry) => <tr key={entry.id} className="border-t border-[var(--hairline)]"><td className="p-3">{t(`merchantPool.entryKinds.${entry.kind}`, { defaultValue: entry.kind })}</td><td className="p-3 tabular-nums">{formatQuota(entry.gross_quota)}</td><td className="p-3 tabular-nums">{formatQuota(entry.commission_quota)}</td><td className="p-3 tabular-nums">{formatQuota(entry.net_quota)}</td><td className="whitespace-nowrap p-3">{poolTimestamp(entry.created_at, i18n.language)}</td></tr>)}</tbody></table></div>}<div className="flex justify-end gap-2"><Button size="sm" variant="flat" isDisabled={previousCursors.length === 0 || entries.isFetching} onPress={() => { setCursor(previousCursors.at(-1)); setPreviousCursors((current) => current.slice(0, -1)) }}>{t('merchantPool.previous')}</Button><Button size="sm" variant="flat" isDisabled={!entries.data?.next_cursor || entries.isFetching} onPress={() => { setPreviousCursors((current) => [...current, cursor]); setCursor(entries.data?.next_cursor ?? undefined) }}>{t('merchantPool.next')}</Button></div></div>
  </section>
}
