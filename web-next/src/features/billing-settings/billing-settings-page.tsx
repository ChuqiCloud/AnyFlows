import { Button, Skeleton } from '@heroui/react'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useAdminBalanceAlertSettings } from './billing-settings-api'
import { BillingSettingsForm } from './billing-settings-form'

/** 呈现管理员计费与余额预警设置。 */
export function BillingSettingsPage() {
  const { t } = useTranslation()
  const settingsQuery = useAdminBalanceAlertSettings()

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('billingSettings.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('billingSettings.subtitle')}</p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="bordered"
          isDisabled={settingsQuery.isFetching}
          onClick={() => void settingsQuery.refetch()}
        >
          <RefreshCw className={settingsQuery.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
          {t('billingSettings.actions.refresh')}
        </Button>
      </header>

      {settingsQuery.isPending ? (
        <div className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_19rem]" aria-label={t('billingSettings.loading')}>
          <div className="overflow-hidden rounded-xl border border-[var(--hairline)]">
            <div className="flex items-center justify-between border-b border-[var(--hairline)] p-5">
              <Skeleton className="h-5 w-44" />
              <Skeleton className="h-5 w-16" />
            </div>
            {[0, 1, 2, 3, 4].map((item) => (
              <div key={item} className="grid gap-3 border-b border-[var(--hairline)] px-5 py-4 last:border-b-0 md:grid-cols-2">
                <div className="grid gap-2"><Skeleton className="h-4 w-32" /><Skeleton className="h-3 w-64 max-w-full" /></div>
                <Skeleton className="h-8 w-full md:ml-auto md:max-w-xs" />
              </div>
            ))}
          </div>
          <Skeleton className="h-56 rounded-xl" />
        </div>
      ) : settingsQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('billingSettings.errors.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('billingSettings.errors.load')}</p>
          <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void settingsQuery.refetch()}>
            {t('billingSettings.actions.retry')}
          </Button>
        </div>
      ) : settingsQuery.data ? (
        <BillingSettingsForm settings={settingsQuery.data} />
      ) : null}
    </div>
  )
}
