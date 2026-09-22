import { Button, Skeleton } from '@heroui/react'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useAdminSiteSettings } from '@/features/site-settings/site-settings-api'
import { useAdminPaymentSettings } from './payment-settings-api'
import { PaymentSettingsWorkspace } from './payment-settings-workspace'

/** 呈现 Stripe 与易支付的结构化管理配置。 */
export function PaymentSettingsPage() {
  const { t } = useTranslation()
  const settingsQuery = useAdminPaymentSettings()
  const siteSettingsQuery = useAdminSiteSettings()
  const isFetching = settingsQuery.isFetching || siteSettingsQuery.isFetching
  const refresh = () => {
    void settingsQuery.refetch()
    void siteSettingsQuery.refetch()
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('paymentSettings.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('paymentSettings.subtitle')}</p>
        </div>
        <Button type="button" size="sm" variant="bordered" isDisabled={isFetching} onClick={refresh}>
          <RefreshCw className={isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
          {t('paymentSettings.actions.refresh')}
        </Button>
      </header>

      {settingsQuery.isPending || siteSettingsQuery.isPending ? (
        <div className="grid gap-4 lg:grid-cols-2" aria-label={t('paymentSettings.loading')}>
          {[0, 1].map((item) => (
            <div key={item} className="overflow-hidden rounded-xl border border-[var(--hairline)]">
              <div className="flex items-center justify-between border-b border-[var(--hairline)] p-4"><Skeleton className="h-5 w-36" /><Skeleton className="h-5 w-16" /></div>
              {[0, 1, 2, 3].map((row) => <div key={row} className="grid gap-2 border-b border-[var(--hairline)] px-4 py-3 last:border-b-0"><Skeleton className="h-4 w-28" /><Skeleton className="h-8 w-full" /></div>)}
            </div>
          ))}
        </div>
      ) : settingsQuery.isError || siteSettingsQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('paymentSettings.errors.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('paymentSettings.errors.load')}</p>
          <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={refresh}>{t('paymentSettings.actions.retry')}</Button>
        </div>
      ) : settingsQuery.data && siteSettingsQuery.data ? (
        <PaymentSettingsWorkspace
          settings={settingsQuery.data}
          publicBaseUrl={siteSettingsQuery.data.public_base_url}
        />
      ) : null}
    </div>
  )
}
