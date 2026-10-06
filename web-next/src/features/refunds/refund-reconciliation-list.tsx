import { Button, Card, CardBody, CardHeader, Chip, Skeleton } from '@heroui/react'
import { ArrowDownRight, CircleAlert, FileCheck2, RefreshCw } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useMemo, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import type { RefundReconciliationEntry } from '@/lib/api/generated/types.gen'
import { useRefundReconciliations, type RefundReconciliationScope } from './refund-reconciliation-api'

type ReconciliationTranslationPrefix = 'wallet.reconciliation' | 'organizationConsole.wallet.reconciliation' | 'refunds.reconciliation'

/** 展示脱敏的现金退款成功事实，不渲染 Provider 交易号或原始回执。 */
export function RefundReconciliationList({
  scope,
  organizationId,
  translationPrefix,
  enabled = true,
}: {
  scope: RefundReconciliationScope
  organizationId?: number
  translationPrefix: ReconciliationTranslationPrefix
  enabled?: boolean
}) {
  const { i18n, t } = useTranslation()
  const query = useRefundReconciliations(scope, organizationId, enabled)
  const entries = query.data?.pages.flatMap((page) => page.entries) ?? []
  const locale = i18n.resolvedLanguage === 'zh' ? 'zh-CN' : 'en-US'
  const timeFormat = useMemo(() => new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }), [locale])
  const refreshing = query.isFetching && !query.isFetchingNextPage

  return (
    <Card className="overflow-hidden border border-[var(--hairline)]" shadow="none">
      <CardHeader className="flex-col items-stretch gap-1.5 border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <FileCheck2 className="size-4 text-brand" aria-hidden="true" />
              <h3 className="text-base font-semibold">{t(`${translationPrefix}.title`)}</h3>
            </div>
            <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t(`${translationPrefix}.subtitle`)}</p>
          </div>
          <Button
            isIconOnly
            aria-label={t(`${translationPrefix}.actions.refresh`)}
            isDisabled={refreshing}
            size="sm"
            title={t(`${translationPrefix}.actions.refresh`)}
            type="button"
            variant="light"
            onClick={() => void query.refetch()}
          >
            <RefreshCw className={refreshing ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
          </Button>
        </div>
      </CardHeader>
      <CardBody className="p-0">
        {query.isPending ? (
          <ReconciliationLoading label={t(`${translationPrefix}.loading`)} />
        ) : query.isError && entries.length === 0 ? (
          <ReconciliationState
            icon={CircleAlert}
            title={t(`${translationPrefix}.errors.title`)}
            body={t(`${translationPrefix}.errors.body`)}
            action={<Button type="button" size="sm" variant="bordered" onClick={() => void query.refetch()}><RefreshCw className="size-3.5" aria-hidden="true" />{t(`${translationPrefix}.actions.retry`)}</Button>}
          />
        ) : entries.length === 0 ? (
          <ReconciliationState icon={FileCheck2} title={t(`${translationPrefix}.emptyTitle`)} body={t(`${translationPrefix}.emptyBody`)} />
        ) : (
          <>
            <div className="overflow-x-auto">
              <table className="w-full min-w-[860px] text-left text-xs">
                <thead className="bg-surface-2/45 text-muted-foreground">
                  <tr>
                    <th className="px-4 py-2 font-medium">{t(`${translationPrefix}.columns.owner`)}</th>
                    <th className="px-4 py-2 font-medium">{t(`${translationPrefix}.columns.order`)}</th>
                    <th className="px-4 py-2 font-medium">{t(`${translationPrefix}.columns.amount`)}</th>
                    <th className="px-4 py-2 font-medium">{t(`${translationPrefix}.columns.status`)}</th>
                    <th className="px-4 py-2 font-medium">{t(`${translationPrefix}.columns.approver`)}</th>
                    <th className="px-4 py-2 font-medium">{t(`${translationPrefix}.columns.time`)}</th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry) => (
                    <ReconciliationRow key={entry.request_id} entry={entry} scope={scope} timeFormat={timeFormat} translationPrefix={translationPrefix} locale={locale} />
                  ))}
                </tbody>
              </table>
            </div>
            {query.isFetchNextPageError ? (
              <div role="alert" className="m-3 flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
                <span>{t(`${translationPrefix}.errors.more`)}</span>
                <Button type="button" size="sm" variant="light" onClick={() => void query.fetchNextPage()}>{t(`${translationPrefix}.actions.retry`)}</Button>
              </div>
            ) : null}
            {query.hasNextPage && !query.isFetchNextPageError ? (
              <div className="flex justify-center border-t border-[var(--hairline)] p-4">
                <Button type="button" size="sm" variant="bordered" isDisabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>
                  <RefreshCw className={query.isFetchingNextPage ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
                  {t(`${translationPrefix}.actions.${query.isFetchingNextPage ? 'loadingMore' : 'loadMore'}`)}
                </Button>
              </div>
            ) : null}
          </>
        )}
      </CardBody>
    </Card>
  )
}

function ReconciliationRow({ entry, scope, timeFormat, translationPrefix, locale }: { entry: RefundReconciliationEntry; scope: RefundReconciliationScope; timeFormat: Intl.DateTimeFormat; translationPrefix: ReconciliationTranslationPrefix; locale: string }) {
  const { t } = useTranslation()
  const owner = entry.organization_id === null ? t(`${translationPrefix}.ownerPersonal`) : t(`${translationPrefix}.ownerOrganization`, { id: entry.organization_id })
  return (
    <tr className="border-t border-[var(--hairline)] align-top">
      <td className="px-4 py-3"><div className="font-medium">{owner}</div><div className="mt-1 text-muted-foreground">{t(`${translationPrefix}.user`, { id: entry.user_id })}</div></td>
      <td className="px-4 py-3"><div className="font-medium">{t(`refunds.orderKinds.${entry.order_kind}`)}</div><div className="mt-1 max-w-[190px] truncate font-mono text-muted-foreground" title={entry.order_key}>{entry.order_key}</div><div className="mt-1 text-muted-foreground">{entry.provider} · {entry.currency}</div></td>
      <td className="px-4 py-3"><div className="flex items-center gap-1.5 font-mono font-semibold text-destructive tabular-nums"><ArrowDownRight className="size-3.5" aria-hidden="true" />{formatMinorAmount(entry.amount_delta_minor, entry.currency, locale)}</div><div className="mt-1 text-[0.6875rem] text-muted-foreground">{t(`${translationPrefix}.minorUnit`)}</div></td>
      <td className="px-4 py-3"><Chip className="text-success" size="sm" variant="flat">{t('refunds.statuses.succeeded')}</Chip></td>
      <td className="px-4 py-3 text-muted-foreground">{t(`${translationPrefix}.approver`, { id: entry.approval_actor_id })}</td>
      <td className="whitespace-nowrap px-4 py-3 text-muted-foreground">{timeFormat.format(entry.created_at * 1_000)}<div className="mt-1 text-[0.625rem]">#{entry.request_id.slice(0, 8)}{scope === 'admin' ? ` · ${t(`${translationPrefix}.adminScope`)}` : ''}</div></td>
    </tr>
  )
}

function formatMinorAmount(value: number, currency: string, locale: string) {
  try {
    return new Intl.NumberFormat(locale, { style: 'currency', currency: currency.toUpperCase(), currencyDisplay: 'code' }).format(value / 100)
  } catch {
    return `${value.toLocaleString(locale)} ${currency.toUpperCase()}`
  }
}

function ReconciliationLoading({ label }: { label: string }) {
  return <div className="grid gap-3 p-5" role="status" aria-label={label}>{[0, 1, 2, 3].map((item) => <div key={item} className="grid grid-cols-6 gap-3"><Skeleton className="h-4" /><Skeleton className="col-span-2 h-4" /><Skeleton className="h-4" /><Skeleton className="h-4" /><Skeleton className="h-4" /></div>)}</div>
}

function ReconciliationState({ icon: Icon, title, body, action }: { icon: LucideIcon; title: string; body: string; action?: ReactNode }) {
  return <div className="grid min-h-52 place-items-center p-6 text-center"><div className="max-w-sm"><Icon className="mx-auto size-5 text-muted-foreground" aria-hidden="true" /><h3 className="mt-3 text-sm font-semibold">{title}</h3><p className="mt-1 text-xs leading-5 text-muted-foreground">{body}</p>{action ? <div className="mt-4">{action}</div> : null}</div></div>
}
