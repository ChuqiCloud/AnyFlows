import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AppShell } from '@/components/layout/app-shell'
import { LoginPage, type LoginNotice } from '@/features/auth/login-page'
import { SessionBoundary } from '@/features/auth/session-boundary'
import { endManagementSession } from '@/features/auth/session-query'
import { ChannelPage } from '@/features/channels/channel-page'
import { DashboardPage } from '@/features/dashboard/dashboard-page'
import { LandingPage } from '@/features/landing/landing-page'
import { TokenPage } from '@/features/tokens/token-page'
import { UsageLogPage } from '@/features/usage-logs/usage-log-page'
import { SetupGateLoading, SetupGateUnavailable } from '@/features/setup/setup-gate-state'
import { SetupPage } from '@/features/setup/setup-page'
import { useInitialSetupStatus } from '@/features/setup/setup-query'

type AppView = 'channels' | 'tokens' | 'usage-logs' | 'overview' | 'login' | 'setup' | 'landing'

// 临时的 hash 视图切换：TanStack Router 接入前，先让公开页与控制台都可访达。
// 路由落地后此处整体由文件路由替换。
function useHashView(): AppView {
  const [hash, setHash] = useState(() => window.location.hash)

  useEffect(() => {
    const onHashChange = () => setHash(window.location.hash)

    window.addEventListener('hashchange', onHashChange)
    return () => window.removeEventListener('hashchange', onHashChange)
  }, [])

  if (hash.startsWith('#/console/channels')) return 'channels'
  if (hash.startsWith('#/console/tokens')) return 'tokens'
  if (hash.startsWith('#/console/usage-logs')) return 'usage-logs'
  if (hash.startsWith('#/console')) return 'overview'
  if (hash.startsWith('#/login')) return 'login'
  if (hash.startsWith('#/setup')) return 'setup'
  return 'landing'
}

type ManagementEntryProps = {
  view: Exclude<AppView, 'landing'>
  loginNotice?: LoginNotice
  onAuthenticated: () => void
  onLoginRequired: (reason: 'adminRequired' | 'required' | 'sessionExpired' | 'setupComplete') => void
  onLogout: () => void
}

/** 在登录或控制台挂载前完成首次安装状态判定。 */
function ManagementEntry({
  view,
  loginNotice,
  onAuthenticated,
  onLoginRequired,
  onLogout,
}: ManagementEntryProps) {
  const { t } = useTranslation()
  const setupQuery = useInitialSetupStatus()

  useEffect(() => {
    if (setupQuery.data?.setup_required && view !== 'setup') {
      window.location.hash = '#/setup'
    } else if (setupQuery.data && !setupQuery.data.setup_required && view === 'setup') {
      onLoginRequired('required')
    }
  }, [onLoginRequired, setupQuery.data, view])

  if (setupQuery.isPending) {
    return <SetupGateLoading />
  }
  if (setupQuery.isError) {
    return <SetupGateUnavailable onRetry={() => void setupQuery.refetch()} />
  }
  if (setupQuery.data.setup_required) {
    return (
      <SetupPage
        onAuthenticated={onAuthenticated}
        onConflict={() => onLoginRequired('setupComplete')}
      />
    )
  }
  if (view === 'setup') {
    return <SetupGateLoading />
  }
  if (view === 'login') {
    return <LoginPage notice={loginNotice} onAuthenticated={onAuthenticated} />
  }

  const titleKey = view === 'channels'
    ? 'channels.title'
    : view === 'tokens'
      ? 'tokens.title'
      : view === 'usage-logs'
        ? 'usageLogs.title'
        : 'dashboard.title'
  const content = view === 'channels'
    ? <ChannelPage />
    : view === 'tokens'
      ? <TokenPage />
      : view === 'usage-logs'
        ? <UsageLogPage />
        : <DashboardPage />

  return (
    <SessionBoundary onSessionEnded={onLoginRequired}>
      {(session) => (
        <AppShell title={t(titleKey)} currentUser={session.user} onLogout={onLogout}>
          {content}
        </AppShell>
      )}
    </SessionBoundary>
  )
}

export default function App() {
  const view = useHashView()
  const [loginNotice, setLoginNotice] = useState<LoginNotice>()

  const openLogin = useCallback((reason: 'adminRequired' | 'required' | 'sessionExpired' | 'setupComplete') => {
    setLoginNotice(reason === 'required' ? undefined : reason)
    window.location.hash = '#/login'
  }, [])

  const openConsole = useCallback(() => {
    setLoginNotice(undefined)
    window.location.hash = '#/console'
  }, [])

  const logout = useCallback(() => {
    endManagementSession()
    setLoginNotice(undefined)
    window.location.hash = '#/login'
  }, [])

  if (view === 'landing') {
    return <LandingPage />
  }

  return (
    <ManagementEntry
      view={view}
      loginNotice={loginNotice}
      onAuthenticated={openConsole}
      onLoginRequired={openLogin}
      onLogout={logout}
    />
  )
}
