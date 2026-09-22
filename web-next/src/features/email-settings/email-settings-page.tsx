import { Button, Skeleton } from '@heroui/react'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import { useAdminEmailSettings } from './email-settings-api'
import { EmailSettingsWorkspace } from './email-settings-workspace'

/** 呈现系统邮件设置，并在固定记录不可用时保持失败关闭。 */
export function EmailSettingsPage() {
  const { t } = useTranslation()
  const settingsQuery = useAdminEmailSettings()

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('emailSettings.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('emailSettings.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="bordered"
          isDisabled={settingsQuery.isFetching}
          onClick={() => void settingsQuery.refetch()}
        >
          <RefreshCw className={cn('size-3.5', settingsQuery.isFetching && 'animate-spin')} aria-hidden="true" />
          {t('emailSettings.actions.refresh')}
        </Button>
      </header>

      {settingsQuery.isPending ? (
        <div
          className="grid items-start gap-4 lg:grid-cols-[minmax(0,1.45fr)_minmax(16rem,0.75fr)]"
          aria-label={t('emailSettings.loading')}
        >
          <div className="overflow-hidden rounded-xl border border-[var(--hairline)]">
            <div className="flex items-center justify-between border-b border-[var(--hairline)] p-4">
              <Skeleton className="h-5 w-44" />
              <Skeleton className="h-5 w-9 rounded-full" />
            </div>
            <div className="grid gap-4 p-4 sm:grid-cols-2">
              {[0, 1, 2, 3, 4, 5].map((item) => (
                <div key={item} className="grid gap-2">
                  <Skeleton className="h-3 w-24" />
                  <Skeleton className="h-8 w-full" />
                </div>
              ))}
            </div>
          </div>
          <Skeleton className="h-64 rounded-xl" />
        </div>
      ) : settingsQuery.isError ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('emailSettings.errors.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('emailSettings.errors.load')}</p>
          <Button
            type="button"
            size="sm"
            variant="bordered"
            className="mt-3"
            onClick={() => void settingsQuery.refetch()}
          >
            {t('emailSettings.actions.retry')}
          </Button>
        </div>
      ) : settingsQuery.data ? (
        <EmailSettingsWorkspace settings={settingsQuery.data} />
      ) : null}
    </div>
  )
}
