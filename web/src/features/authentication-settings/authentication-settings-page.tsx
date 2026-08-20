import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Skeleton } from '@/components/ui/skeleton'
import { useAdminGroupCatalog } from '@/features/groups/group-api'
import { AuthenticationSettingsForm } from './authentication-settings-form'
import { DiscordOAuthSettings } from './discord-oauth-settings'
import { GitHubOAuthSettings } from './github-oauth-settings'
import { OAuthProviderSettings } from './oauth-provider-settings'
import { useAdminAuthenticationSettings } from './authentication-settings-api'

/** 呈现管理员认证设置，并在策略或分组目录不可靠时禁止编辑。 */
export function AuthenticationSettingsPage() {
  const { t } = useTranslation()
  const settingsQuery = useAdminAuthenticationSettings()
  const groupsQuery = useAdminGroupCatalog()
  const pending = settingsQuery.isPending || groupsQuery.isPending
  const failed = settingsQuery.isError || groupsQuery.isError || groupsQuery.data?.length === 0

  const refresh = () => {
    void settingsQuery.refetch()
    void groupsQuery.refetch()
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-lg font-semibold">{t('authenticationSettings.title')}</h2>
          <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">
            {t('authenticationSettings.subtitle')}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          variant="secondary"
          disabled={settingsQuery.isFetching || groupsQuery.isFetching}
          onClick={refresh}
        >
          <RefreshCw
            className={settingsQuery.isFetching || groupsQuery.isFetching ? 'animate-spin' : undefined}
            aria-hidden="true"
          />
          {t('authenticationSettings.actions.refresh')}
        </Button>
      </header>

      {pending ? (
        <div className="grid gap-0 border-y border-[var(--hairline)]" aria-label={t('authenticationSettings.loading')}>
          {[0, 1, 2, 3, 4].map((item) => (
            <div key={item} className="grid gap-3 border-b border-[var(--hairline)] py-5 last:border-b-0 md:grid-cols-2">
              <div className="grid gap-2"><Skeleton className="h-4 w-32" /><Skeleton className="h-3 w-64 max-w-full" /></div>
              <Skeleton className="h-9 w-full" />
            </div>
          ))}
        </div>
      ) : failed ? (
        <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
          <h3 className="text-sm font-semibold text-destructive">{t('authenticationSettings.errors.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t(groupsQuery.data?.length === 0
              ? 'authenticationSettings.errors.groupsEmpty'
              : 'authenticationSettings.errors.load')}
          </p>
          <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={refresh}>
            {t('authenticationSettings.actions.retry')}
          </Button>
        </div>
      ) : settingsQuery.data && groupsQuery.data ? (
        <>
          <AuthenticationSettingsForm settings={settingsQuery.data} groups={groupsQuery.data} />
          <GitHubOAuthSettings />
          <DiscordOAuthSettings />
          <OAuthProviderSettings provider="oidc" />
          <OAuthProviderSettings provider="linuxdo" />
          <OAuthProviderSettings provider="wechat" />
          <OAuthProviderSettings provider="telegram" />
        </>
      ) : null}
    </div>
  )
}
