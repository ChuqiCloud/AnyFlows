import { Activity, Gauge, UserRound } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminUser } from '@/features/users/user-api'
import { cn } from '@/lib/utils'
import { formatUsageNumber } from './usage-log-format'

type UsageLogUserSheetProps = {
  userId?: number
  formatQuota: (value: number) => string
  onOpenChange: (open: boolean) => void
}

/** 从调用日志按需读取管理员可见的用户与钱包摘要。 */
export function UsageLogUserSheet({ userId, formatQuota, onOpenChange }: UsageLogUserSheetProps) {
  const { i18n, t } = useTranslation()
  const query = useAdminUser(userId)
  const user = query.data
  const number = (value: number) => formatUsageNumber(value, i18n.language)

  return (
    <Sheet open={userId !== undefined} onOpenChange={onOpenChange}>
      <SheetContent className="gap-0 overflow-hidden data-[side=right]:w-full data-[side=right]:sm:max-w-xl" aria-describedby="usage-log-user-description">
        <SheetHeader className="border-b border-[var(--hairline)] px-5 py-4 pr-12">
          <div className="flex min-w-0 items-center gap-2">
            <UserRound className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
            <SheetTitle className="truncate">{user?.username ?? t('usageLogs.userSheet.title')}</SheetTitle>
            {user ? <Badge className={cn('shrink-0 border-transparent', user.status === 'enabled' ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground')}>{t(`users.status.${user.status}`)}</Badge> : null}
          </div>
          <SheetDescription id="usage-log-user-description">{t('usageLogs.userSheet.description')}</SheetDescription>
        </SheetHeader>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {query.isPending ? (
            <div className="grid gap-3 p-5" aria-label={t('usageLogs.userSheet.loading')}>
              <Skeleton className="h-20 rounded-lg" />
              <Skeleton className="h-40 rounded-lg" />
            </div>
          ) : query.isError || !user ? (
            <div className="m-5 border border-destructive/25 bg-destructive/8 p-4" role="alert">
              <h3 className="text-sm font-semibold text-destructive">{t('usageLogs.userSheet.errorTitle')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('usageLogs.userSheet.errorBody')}</p>
              <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void query.refetch()}>{t('usageLogs.actions.retry')}</Button>
            </div>
          ) : (
            <>
              <div className="grid grid-cols-3 divide-x divide-[var(--hairline)] border-b border-[var(--hairline)] bg-surface-2/35">
                <Metric label={t('users.wallet.metrics.available')} value={formatQuota(user.quota)} />
                <Metric label={t('users.wallet.metrics.used')} value={formatQuota(user.used_quota)} />
                <Metric label={t('users.wallet.metrics.frozen')} value={formatQuota(user.frozen_quota)} />
              </div>

              <section className="border-b border-[var(--hairline)] px-5 py-5">
                <h3 className="flex items-center gap-2 text-xs font-semibold"><UserRound className="size-3.5 text-muted-foreground" aria-hidden="true" />{t('usageLogs.userSheet.account')}</h3>
                <dl className="mt-3 grid grid-cols-2 gap-x-6 gap-y-4 text-sm">
                  <Meta label={t('users.fields.username')} value={user.username} />
                  <Meta label={t('users.fields.email')} value={user.email ?? t('users.values.noEmail')} />
                  <Meta label={t('users.fields.role')} value={t(`users.role.${user.role}`)} />
                  <Meta label={t('users.fields.defaultGroup')} value={`#${user.default_group_id}`} />
                </dl>
              </section>

              <section className="px-5 py-5">
                <h3 className="flex items-center gap-2 text-xs font-semibold"><Gauge className="size-3.5 text-muted-foreground" aria-hidden="true" />{t('usageLogs.userSheet.traffic')}</h3>
                <dl className="mt-3 grid grid-cols-2 gap-x-6 gap-y-4 text-sm">
                  <Meta label={t('usageLogs.userSheet.requests')} value={number(user.request_count)} />
                  <Meta label={t('users.fields.rpmLimit')} value={user.rpm_limit === null ? t('users.values.unlimited') : number(user.rpm_limit)} />
                  <Meta label={t('users.fields.concurrency')} value={user.concurrency === null ? t('users.values.unlimited') : number(user.concurrency)} />
                  <Meta label={t('usageLogs.detailFields.user')} value={`#${user.id}`} />
                </dl>
                <Button asChild type="button" size="sm" variant="secondary" className="mt-5">
                  <a href="#/console/users"><Activity aria-hidden="true" />{t('usageLogs.userSheet.openManagement')}</a>
                </Button>
              </section>
            </>
          )}
        </div>
      </SheetContent>
    </Sheet>
  )
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="min-w-0 px-3 py-3 text-center"><div className="truncate text-[0.625rem] text-muted-foreground">{label}</div><div className="mt-1 truncate text-sm font-semibold tabular-nums" title={value}>{value}</div></div>
}

function Meta({ label, value }: { label: string; value: string }) {
  return <div className="min-w-0"><dt className="text-[0.6875rem] text-muted-foreground">{label}</dt><dd className="mt-1 break-words tabular-nums" title={value}>{value}</dd></div>
}
