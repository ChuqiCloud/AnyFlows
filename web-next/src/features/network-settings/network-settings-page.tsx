import { Button, Skeleton } from '@heroui/react'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useAdminNetworkSettings } from './network-settings-api'
import { NetworkSettingsWorkspace } from './network-settings-workspace'

/** 呈现管理员网络与代理设置，并在固定记录不可用时保持失败关闭。 */
export function NetworkSettingsPage() {
  const { t } = useTranslation()
  const settingsQuery = useAdminNetworkSettings()

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('networkSettings.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('networkSettings.subtitle')}</p>
        </div>
        <Button type="button" size="sm" variant="bordered" isDisabled={settingsQuery.isFetching} onClick={() => void settingsQuery.refetch()}>
          <RefreshCw className={settingsQuery.isFetching ? 'size-3.5 animate-spin' : 'size-3.5'} aria-hidden="true" />
          {t('networkSettings.actions.refresh')}
        </Button>
      </header>

      {settingsQuery.isPending ? (
        <div className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_19rem]" aria-label={t('networkSettings.loading')}>
          <div className="overflow-hidden rounded-xl border border-[var(--hairline)]">
            <div className="flex items-center justify-between border-b border-[var(--hairline)] p-5"><Skeleton className="h-5 w-44" /><Skeleton className="h-5 w-20" /></div>
            {[0, 1, 2, 3].map((item) => <div key={item} className="grid gap-2 border-b border-[var(--hairline)] px-5 py-4"><Skeleton className="h-4 w-32" /><Skeleton className="h-8 w-full" /></div>)}
          </div>
          <Skeleton className="h-56 rounded-xl" />
        </div>
      ) : settingsQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('networkSettings.errors.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('networkSettings.errors.load')}</p>
          <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={() => void settingsQuery.refetch()}>{t('networkSettings.actions.retry')}</Button>
        </div>
      ) : settingsQuery.data ? <NetworkSettingsWorkspace settings={settingsQuery.data} /> : null}
    </div>
  )
}
