import { Button, Chip, Input, Select, SelectItem } from '@heroui/react'
import { AlertTriangle, Check, ChevronDown, CircleDollarSign, FileText, Landmark, LoaderCircle, Plus, RefreshCw, ShieldX } from 'lucide-react'
import { useMemo, useState, type FormEvent, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { OrganizationPageBoundary, OrganizationPageHeader, OrganizationPageState, useOrganizationPageState } from '@/features/organization-console/organization-page-shell'
import type { CreditInvoiceResponse, CreditRepaymentResponse, CreditAllocationResponse, CreditTermResponse } from '@/lib/api/generated/types.gen'
import {
  organizationCreditErrorStatus,
  useCreateOrganizationCreditTerm,
  useOrganizationCreditAllocations,
  useOrganizationCreditInvoices,
  useOrganizationCreditRepayments,
  useOrganizationCreditTerms,
  useUpdateOrganizationCreditTermStatus,
} from './organization-credit-api'

const CREDIT_READ = 'organization.credit.read'
const CREDIT_MANAGE = 'organization.credit.manage'
type CreditTab = 'terms' | 'invoices' | 'repayments' | 'allocations'

/** 企业授信闭环工作区；账务事实只读，策略写入通过服务端 CAS 完成。 */
export function OrganizationCreditPage() {
  const { t } = useTranslation()
  const state = useOrganizationPageState()
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <OrganizationPageHeader icon={Landmark} title={t('organizationCredit.title')} subtitle={t('organizationCredit.subtitle')} />
      <OrganizationPageBoundary state={state}>
        {(organizationId, overview) => <OrganizationCreditWorkspace key={organizationId} organizationId={organizationId} canRead={overview.permissions.includes(CREDIT_READ)} canManage={overview.permissions.includes(CREDIT_MANAGE)} />}
      </OrganizationPageBoundary>
    </div>
  )
}

function OrganizationCreditWorkspace({ organizationId, canRead, canManage }: { organizationId: number; canRead: boolean; canManage: boolean }) {
  const { t, i18n } = useTranslation()
  const [tab, setTab] = useState<CreditTab>('terms')
  const terms = useOrganizationCreditTerms(organizationId, canRead && tab === 'terms')
  const invoices = useOrganizationCreditInvoices(organizationId, canRead && tab === 'invoices')
  const repayments = useOrganizationCreditRepayments(organizationId, canRead && tab === 'repayments')
  const allocations = useOrganizationCreditAllocations(organizationId, canRead && tab === 'allocations')
  const createMutation = useCreateOrganizationCreditTerm(organizationId)
  const statusMutation = useUpdateOrganizationCreditTermStatus(organizationId)
  const [createOpen, setCreateOpen] = useState(false)
  const formatTime = useMemo(() => new Intl.DateTimeFormat(i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }), [i18n.resolvedLanguage])

  if (!canRead) return <OrganizationPageState icon={ShieldX} title={t('organizationCredit.restricted.title')} body={t('organizationCredit.restricted.body')} />

  const activeQuery = tab === 'terms' ? terms : tab === 'invoices' ? invoices : tab === 'repayments' ? repayments : allocations
  const refresh = () => void activeQuery.refetch()
  const errorStatus = organizationCreditErrorStatus(activeQuery.error ?? createMutation.error ?? statusMutation.error)
  const tabItems: Array<{ key: CreditTab; icon: typeof Landmark }> = [
    { key: 'terms', icon: Landmark },
    { key: 'invoices', icon: FileText },
    { key: 'repayments', icon: CircleDollarSign },
    { key: 'allocations', icon: Check },
  ]

  return (
    <div className="grid min-w-0 gap-4">
      <div className="flex min-w-0 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] py-3">
        <div role="tablist" aria-label={t('organizationCredit.tabs.label')} className="flex min-w-0 max-w-full gap-1 overflow-x-auto">
          {tabItems.map(({ key, icon: Icon }) => <button key={key} type="button" role="tab" aria-selected={tab === key} className={`inline-flex shrink-0 items-center gap-1.5 rounded-md px-2.5 py-1.5 text-xs ${tab === key ? 'bg-brand/12 text-brand' : 'text-muted-foreground hover:bg-surface-2'}`} onClick={() => setTab(key)}><Icon className="size-3.5" aria-hidden="true" />{t(`organizationCredit.tabs.${key}`)}</button>)}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button type="button" size="sm" variant="bordered" isDisabled={activeQuery.isFetching} onClick={refresh}><RefreshCw className={activeQuery.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />{t('organizationCredit.actions.refresh')}</Button>
          {tab === 'terms' && canManage ? <Button type="button" size="sm" color="primary" onClick={() => { createMutation.reset(); setCreateOpen((value) => !value) }}><Plus className="size-3.5" aria-hidden="true" />{t('organizationCredit.actions.create')}</Button> : null}
        </div>
      </div>

      {!canManage && tab === 'terms' ? <p className="flex items-start gap-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-xs leading-5 text-warning"><ShieldX className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />{t('organizationCredit.readOnly')}</p> : null}
      {createOpen && tab === 'terms' ? <CreditTermCreateForm mutation={createMutation} onDone={() => setCreateOpen(false)} /> : null}
      {activeQuery.isPending ? <CreditLoading /> : activeQuery.isError ? <OrganizationPageState icon={errorStatus === 403 ? ShieldX : AlertTriangle} title={t(errorStatus === 403 ? 'organizationCredit.errors.forbiddenTitle' : 'organizationCredit.errors.loadTitle')} body={t(errorStatus === 403 ? 'organizationCredit.errors.forbiddenBody' : 'organizationCredit.errors.loadBody')} action={<Button type="button" size="sm" variant="bordered" onClick={refresh}><RefreshCw className="size-3.5" aria-hidden="true" />{t('organizationCredit.actions.retry')}</Button>} /> : tab === 'terms' ? <TermsTable pages={terms.data?.pages ?? []} canManage={canManage} statusMutation={statusMutation} formatTime={formatTime} /> : tab === 'invoices' ? <InvoicesTable pages={invoices.data?.pages ?? []} formatTime={formatTime} /> : tab === 'repayments' ? <RepaymentsTable pages={repayments.data?.pages ?? []} formatTime={formatTime} /> : <AllocationsTable pages={allocations.data?.pages ?? []} formatTime={formatTime} />}
      {activeQuery.isSuccess && activeQuery.hasNextPage ? <Button type="button" variant="bordered" className="mx-auto" isDisabled={activeQuery.isFetchingNextPage} onClick={() => void activeQuery.fetchNextPage()}>{activeQuery.isFetchingNextPage ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <ChevronDown className="size-4" aria-hidden="true" />}{t('organizationCredit.actions.loadMore')}</Button> : null}
    </div>
  )
}

function TermsTable({ pages, canManage, statusMutation, formatTime }: { pages: Array<{ terms: CreditTermResponse[] }>; canManage: boolean; statusMutation: ReturnType<typeof useUpdateOrganizationCreditTermStatus>; formatTime: Intl.DateTimeFormat }) {
  const { t } = useTranslation()
  const rows = pages.flatMap((page) => page.terms)
  if (rows.length === 0) return <OrganizationPageState icon={Landmark} title={t('organizationCredit.empty.termsTitle')} body={t('organizationCredit.empty.termsBody')} />
  return <div className="overflow-x-auto rounded-lg border border-[var(--hairline)] bg-surface-1"><table className="w-full min-w-[720px] text-left text-xs"><thead className="bg-surface-2/60 text-muted-foreground"><tr><th className="px-3 py-2 font-medium">{t('organizationCredit.columns.limit')}</th><th className="px-3 py-2 font-medium">{t('organizationCredit.columns.period')}</th><th className="px-3 py-2 font-medium">{t('organizationCredit.columns.status')}</th><th className="px-3 py-2 font-medium">{t('organizationCredit.columns.effective')}</th><th className="px-3 py-2 font-medium">{t('organizationCredit.columns.version')}</th><th className="px-3 py-2" /></tr></thead><tbody className="divide-y divide-[var(--hairline)]">{rows.map((term) => <tr key={term.id} className="hover:bg-surface-2/35"><td className="px-3 py-2.5 font-mono">{term.credit_limit.toLocaleString()} {term.currency}</td><td className="px-3 py-2.5">{term.period.kind === 'net_days' ? t('organizationCredit.period.netDays', { days: term.period.days }) : t('organizationCredit.period.endOfMonth')}</td><td className="px-3 py-2.5"><Chip className={term.status === 'active' ? 'border-primary/40 bg-primary/10 text-primary' : term.status === 'paused' ? 'border-warning/40 bg-warning/10 text-warning' : 'text-muted-foreground'} size="sm" variant="flat">{t(`organizationCredit.status.${term.status}`)}</Chip></td><td className="px-3 py-2.5 text-muted-foreground">{formatTime.format(term.effective_from * 1000)}{term.effective_until ? ` – ${formatTime.format(term.effective_until * 1000)}` : ''}</td><td className="px-3 py-2.5 font-mono text-muted-foreground">v{term.version}</td><td className="px-3 py-2.5 text-right">{canManage && term.status !== 'closed' ? <div className="flex justify-end gap-1"><Button type="button" size="sm" variant="light" isDisabled={statusMutation.isPending} onClick={() => void statusMutation.mutateAsync({ termId: term.id, body: { status: term.status === 'active' ? 'paused' : 'active', expected_version: term.version } })}>{term.status === 'active' ? t('organizationCredit.actions.pause') : t('organizationCredit.actions.resume')}</Button><Button type="button" size="sm" variant="light" className="text-destructive" isDisabled={statusMutation.isPending} onClick={() => void statusMutation.mutateAsync({ termId: term.id, body: { status: 'closed', expected_version: term.version } })}>{t('organizationCredit.actions.close')}</Button></div> : null}</td></tr>)}</tbody></table></div>
}

function InvoicesTable({ pages, formatTime }: { pages: Array<{ invoices: CreditInvoiceResponse[] }>; formatTime: Intl.DateTimeFormat }) {
  const { t } = useTranslation()
  const rows = pages.flatMap((page) => page.invoices)
  if (rows.length === 0) return <OrganizationPageState icon={FileText} title={t('organizationCredit.empty.invoicesTitle')} body={t('organizationCredit.empty.invoicesBody')} />
  return <FactTable headers={[t('organizationCredit.columns.amount'), t('organizationCredit.columns.period'), t('organizationCredit.columns.status'), t('organizationCredit.columns.due')]} rows={rows.map((invoice) => [<span key="amount" className="font-mono">{invoice.amount_minor.toLocaleString()} {invoice.currency}</span>, invoice.period.kind === 'net_days' ? t('organizationCredit.period.netDays', { days: invoice.period.days }) : t('organizationCredit.period.endOfMonth'), <Chip key="status" size="sm" variant="flat">{t(`organizationCredit.invoiceStatus.${invoice.status}`)}</Chip>, formatTime.format(invoice.due_at * 1000)])} />
}

function RepaymentsTable({ pages, formatTime }: { pages: Array<{ repayments: CreditRepaymentResponse[] }>; formatTime: Intl.DateTimeFormat }) {
  const { t } = useTranslation()
  const rows = pages.flatMap((page) => page.repayments)
  if (rows.length === 0) return <OrganizationPageState icon={CircleDollarSign} title={t('organizationCredit.empty.repaymentsTitle')} body={t('organizationCredit.empty.repaymentsBody')} />
  return <FactTable headers={[t('organizationCredit.columns.amount'), t('organizationCredit.columns.source'), t('organizationCredit.columns.received'), t('organizationCredit.columns.created')]} rows={rows.map((repayment) => [<span key="amount" className="font-mono">{repayment.amount_minor.toLocaleString()} {repayment.currency}</span>, t(`organizationCredit.source.${repayment.source_kind}`), formatTime.format(repayment.received_at * 1000), formatTime.format(repayment.created_at * 1000)])} />
}

function AllocationsTable({ pages, formatTime }: { pages: Array<{ allocations: CreditAllocationResponse[] }>; formatTime: Intl.DateTimeFormat }) {
  const { t } = useTranslation()
  const rows = pages.flatMap((page) => page.allocations)
  if (rows.length === 0) return <OrganizationPageState icon={Check} title={t('organizationCredit.empty.allocationsTitle')} body={t('organizationCredit.empty.allocationsBody')} />
  return <FactTable headers={[t('organizationCredit.columns.amount'), t('organizationCredit.columns.invoice'), t('organizationCredit.columns.repayment'), t('organizationCredit.columns.allocated'), t('organizationCredit.columns.settled')]} rows={rows.map((allocation) => [<span key="amount" className="font-mono">{allocation.amount_minor.toLocaleString()} {allocation.currency}</span>, `#${allocation.invoice_id}`, `#${allocation.repayment_id}`, formatTime.format(allocation.allocated_at * 1000), allocation.invoice_settled ? t('organizationCredit.yes') : t('organizationCredit.no')])} />
}

function FactTable({ headers, rows }: { headers: string[]; rows: Array<ReactNode[]> }) {
  return <div className="overflow-x-auto rounded-lg border border-[var(--hairline)] bg-surface-1"><table className="w-full min-w-[640px] text-left text-xs"><thead className="bg-surface-2/60 text-muted-foreground"><tr>{headers.map((header) => <th key={header} className="px-3 py-2 font-medium">{header}</th>)}</tr></thead><tbody className="divide-y divide-[var(--hairline)]">{rows.map((row, index) => <tr key={index} className="hover:bg-surface-2/35">{row.map((cell, cellIndex) => <td key={cellIndex} className="px-3 py-2.5">{cell}</td>)}</tr>)}</tbody></table></div>
}

function CreditTermCreateForm({ mutation, onDone }: { mutation: ReturnType<typeof useCreateOrganizationCreditTerm>; onDone: () => void }) {
  const { t } = useTranslation()
  const [currency, setCurrency] = useState('CNY')
  const [limit, setLimit] = useState('')
  const [period, setPeriod] = useState<'net_days' | 'end_of_month'>('net_days')
  const [days, setDays] = useState('30')
  const [effectiveFrom, setEffectiveFrom] = useState('')
  const periodItems = [
    { key: 'net_days', label: t('organizationCredit.period.netDays', { days }) },
    { key: 'end_of_month', label: t('organizationCredit.period.endOfMonth') },
  ]
  const valid = /^[A-Z]{3}$/.test(currency) && /^[0-9]+$/.test(limit) && Number(limit) >= 0 && Boolean(effectiveFrom) && (period === 'end_of_month' || Number(days) > 0)
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!valid || mutation.isPending) return
    try {
      await mutation.mutateAsync({ currency, credit_limit: Number(limit), period: period === 'net_days' ? { kind: 'net_days', days: Number(days) } : { kind: 'end_of_month' }, effective_from: Math.floor(new Date(effectiveFrom).getTime() / 1000), effective_until: null, expected_version: null })
      onDone()
    } catch {
      // 保留输入，方便用户按服务端错误修正。
    }
  }
  return <form className="grid gap-3 rounded-lg border border-brand/20 bg-brand/4 p-4" onSubmit={submit} noValidate><div className="grid gap-3 sm:grid-cols-4"><label className="grid gap-1 text-xs"><span>{t('organizationCredit.form.currency')}</span><Input inputMode="text" isDisabled={mutation.isPending} maxLength={3} size="sm" value={currency} onChange={(event) => setCurrency(event.target.value.toUpperCase())} /></label><label className="grid gap-1 text-xs"><span>{t('organizationCredit.form.limit')}</span><Input inputMode="numeric" isDisabled={mutation.isPending} size="sm" value={limit} onChange={(event) => setLimit(event.target.value)} /></label><div className="grid gap-1 text-xs"><span id="organization-credit-period-label">{t('organizationCredit.form.period')}</span><Select aria-labelledby="organization-credit-period-label" isDisabled={mutation.isPending} items={periodItems} selectedKeys={[period]} size="sm" onSelectionChange={(keys) => setPeriod(String(Array.from(keys)[0] ?? 'net_days') as typeof period)}>{(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}</Select></div>{period === 'net_days' ? <label className="grid gap-1 text-xs"><span>{t('organizationCredit.form.days')}</span><Input inputMode="numeric" isDisabled={mutation.isPending} size="sm" value={days} onChange={(event) => setDays(event.target.value)} /></label> : <span />}</div><label className="grid max-w-xs gap-1 text-xs"><span>{t('organizationCredit.form.effectiveFrom')}</span><Input isDisabled={mutation.isPending} size="sm" type="datetime-local" value={effectiveFrom} onChange={(event) => setEffectiveFrom(event.target.value)} /></label>{mutation.isError ? <p role="alert" className="text-xs text-destructive">{t('organizationCredit.errors.mutation')}</p> : null}<div className="flex gap-2"><Button type="submit" size="sm" color="primary" isDisabled={!valid || mutation.isPending}>{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Plus className="size-3.5" aria-hidden="true" />}{t('organizationCredit.actions.save')}</Button><Button type="button" size="sm" variant="light" onClick={onDone}>{t('organizationCredit.actions.cancel')}</Button></div></form>
}

function CreditLoading() { return <div className="grid gap-2" role="status">{[0, 1, 2, 3].map((item) => <div key={item} className="h-14 animate-pulse rounded-lg bg-surface-2" />)}</div> }
