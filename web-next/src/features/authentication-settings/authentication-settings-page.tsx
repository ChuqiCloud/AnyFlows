import { Button, Skeleton } from '@heroui/react'
import { LockKeyhole, RefreshCw, ShieldCheck, SlidersHorizontal, Waypoints } from 'lucide-react'
import { useIsFetching, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { adminOAuthLoginSettingsQueryPrefix } from '@/features/auth/oauth-login-api'
import { useAdminGroupCatalog } from '@/features/groups/group-api'
import { AuthenticationSettingsForm } from './authentication-settings-form'
import { DiscordOAuthSettings } from './discord-oauth-settings'
import { GitHubOAuthSettings } from './github-oauth-settings'
import { OAuthProviderSettings } from './oauth-provider-settings'
import { useAdminAuthenticationSettings } from './authentication-settings-api'

const authenticationSections = [
  { id: 'authentication-core', label: 'core', icon: LockKeyhole },
  { id: 'authentication-allocation', label: 'allocation', icon: SlidersHorizontal },
  { id: 'authentication-protection', label: 'protection', icon: ShieldCheck },
  { id: 'authentication-oauth', label: 'oauth', icon: Waypoints },
] as const

/** 呈现管理员认证设置，并在策略或分组目录不可靠时禁止编辑。 */
export function AuthenticationSettingsPage() {
  const { t } = useTranslation()
  const [activeSection, setActiveSection] = useState('authentication-core')
  const settingsQuery = useAdminAuthenticationSettings()
  const groupsQuery = useAdminGroupCatalog()
  const queryClient = useQueryClient()
  const oauthSettingsFetching = useIsFetching({ queryKey: adminOAuthLoginSettingsQueryPrefix }) > 0
  const refreshing = settingsQuery.isFetching || groupsQuery.isFetching || oauthSettingsFetching
  const pending = settingsQuery.isPending || groupsQuery.isPending
  const failed = settingsQuery.isError || groupsQuery.isError || groupsQuery.data?.length === 0
  const sectionsReady = Boolean(settingsQuery.data && groupsQuery.data)

  const refresh = () => {
    void settingsQuery.refetch()
    void groupsQuery.refetch()
    void queryClient.invalidateQueries({ queryKey: adminOAuthLoginSettingsQueryPrefix })
  }

  useEffect(() => {
    const updateActiveSection = () => {
      const visible = authenticationSections
        .flatMap((section) => {
          const element = document.getElementById(section.id)
          if (!element) return []
          const bounds = element.getBoundingClientRect()
          return bounds.bottom > 80
            ? [{ id: section.id, distance: Math.abs(bounds.top - 96) }]
            : []
        })
        .sort((left, right) => left.distance - right.distance)[0]
      if (visible) {
        setActiveSection((current) => (current === visible.id ? current : visible.id))
      }
    }

    updateActiveSection()
    window.addEventListener('scroll', updateActiveSection, { passive: true })
    window.addEventListener('resize', updateActiveSection)
    return () => {
      window.removeEventListener('scroll', updateActiveSection)
      window.removeEventListener('resize', updateActiveSection)
    }
  }, [sectionsReady])

  const focusSection = (id: string) => {
    const element = document.getElementById(id)
    if (!element) return
    setActiveSection(id)
    element.focus({ preventScroll: true })
    element.scrollIntoView({ behavior: 'smooth', block: 'start' })
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
          variant="bordered"
          isDisabled={refreshing}
          onClick={refresh}
        >
          <RefreshCw
            className={refreshing ? 'size-3.5 animate-spin' : 'size-3.5'}
            aria-hidden="true"
          />
          {t('authenticationSettings.actions.refresh')}
        </Button>
      </header>

      <nav
        aria-label={t('authenticationSettings.navigation.label')}
        className="-mx-1 flex gap-1 overflow-x-auto px-1 pb-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
      >
        {authenticationSections.map((section) => {
          const Icon = section.icon
          const active = activeSection === section.id
          return (
            <button
              key={section.id}
              type="button"
              aria-current={active ? 'true' : undefined}
              className={active
                ? 'flex h-8 shrink-0 items-center gap-2 rounded-lg bg-surface-2 px-3 text-xs font-medium text-foreground ring-1 ring-[var(--hairline)]'
                : 'flex h-8 shrink-0 items-center gap-2 rounded-lg px-3 text-xs font-medium text-muted-foreground transition-colors hover:bg-surface-2/70 hover:text-foreground'}
              onClick={() => focusSection(section.id)}
            >
              <Icon className={active ? 'size-3.5 text-brand' : 'size-3.5'} aria-hidden="true" />
              {t('authenticationSettings.navigation.' + section.label)}
            </button>
          )
        })}
      </nav>

      {pending ? (
        <div className="grid gap-0 border-t border-[var(--hairline)]" aria-label={t('authenticationSettings.loading')}>
          {[0, 1, 2, 3, 4, 5].map((item) => (
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
          <Button type="button" size="sm" variant="bordered" className="mt-3" onClick={refresh}>
            {t('authenticationSettings.actions.retry')}
          </Button>
        </div>
      ) : settingsQuery.data && groupsQuery.data ? (
        <>
          <AuthenticationSettingsForm settings={settingsQuery.data} groups={groupsQuery.data} />
          <section id="authentication-oauth" tabIndex={-1} className="grid scroll-mt-20 gap-4 rounded-2xl outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
            <GitHubOAuthSettings />
            <DiscordOAuthSettings />
            <OAuthProviderSettings provider="oidc" />
            <OAuthProviderSettings provider="linuxdo" />
            <OAuthProviderSettings provider="wechat" />
            <OAuthProviderSettings provider="telegram" />
            <OAuthProviderSettings provider="google" />
          </section>
        </>
      ) : null}
    </div>
  )
}
