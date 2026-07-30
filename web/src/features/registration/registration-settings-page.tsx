import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminGroupCatalog } from '@/features/groups/group-api'
import { useAdminRegistrationPolicy } from './registration-api'
import { RegistrationSettingsForm } from './registration-settings-form'

/** 呈现管理员注册策略，并在策略或分组目录不可靠时禁止编辑。 */
export function RegistrationSettingsPage() {
  const { t } = useTranslation()
  const policyQuery = useAdminRegistrationPolicy()
  const groupsQuery = useAdminGroupCatalog()
  const pending = policyQuery.isPending || groupsQuery.isPending
  const failed = policyQuery.isError || groupsQuery.isError || groupsQuery.data?.length === 0

  const refresh = () => {
    void policyQuery.refetch()
    void groupsQuery.refetch()
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('registration.settings.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('registration.settings.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={policyQuery.isFetching || groupsQuery.isFetching}
          onClick={refresh}
        >
          <RefreshCw
            className={policyQuery.isFetching || groupsQuery.isFetching ? 'animate-spin' : undefined}
            aria-hidden="true"
          />
          {t('registration.settings.actions.refresh')}
        </Button>
      </header>

      {pending ? (
        <div className="grid gap-0 border-y border-[var(--hairline)]" aria-label={t('registration.settings.loading')}>
          {[0, 1, 2, 3].map((item) => (
            <div key={item} className="grid gap-3 border-b border-[var(--hairline)] py-5 last:border-b-0 md:grid-cols-2">
              <div className="grid gap-2"><Skeleton className="h-4 w-32" /><Skeleton className="h-3 w-64 max-w-full" /></div>
              <Skeleton className="h-9 w-full" />
            </div>
          ))}
        </div>
      ) : failed ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('registration.settings.errors.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t(groupsQuery.data?.length === 0
              ? 'registration.settings.errors.groupsEmpty'
              : 'registration.settings.errors.load')}
          </p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={refresh}>
            {t('registration.settings.actions.retry')}
          </Button>
        </div>
      ) : policyQuery.data && groupsQuery.data ? (
        <RegistrationSettingsForm policy={policyQuery.data} groups={groupsQuery.data} />
      ) : null}
    </div>
  )
}
