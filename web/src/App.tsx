import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AppShell } from '@/components/layout/app-shell'
import { LoginPage, type LoginNotice } from '@/features/auth/login-page'
import { SessionBoundary } from '@/features/auth/session-boundary'
import { endManagementSession } from '@/features/auth/session-query'
import { ChannelPage } from '@/features/channels/channel-page'
import { DashboardPage } from '@/features/dashboard/dashboard-page'
import { LandingPage } from '@/features/landing/landing-page'

// 临时的 hash 视图切换：TanStack Router 接入前，先让公开页与控制台都可访达。
// 路由落地后此处整体由文件路由替换。
function useHashView() {
  const [hash, setHash] = useState(() => window.location.hash)

  useEffect(() => {
    const onHashChange = () => setHash(window.location.hash)

    window.addEventListener('hashchange', onHashChange)
    return () => window.removeEventListener('hashchange', onHashChange)
  }, [])

  if (hash.startsWith('#/console/channels')) return 'channels'
  if (hash.startsWith('#/console')) return 'overview'
  if (hash.startsWith('#/login')) return 'login'
  return 'landing'
}

export default function App() {
  const { t } = useTranslation()
  const view = useHashView()
  const [loginNotice, setLoginNotice] = useState<LoginNotice>()

  const openLogin = useCallback((reason: 'adminRequired' | 'required' | 'sessionExpired') => {
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

  if (view === 'login') {
    return <LoginPage notice={loginNotice} onAuthenticated={openConsole} />
  }

  const channelView = view === 'channels'
  return (
    <SessionBoundary onSessionEnded={openLogin}>
      {(session) => (
        <AppShell title={t(channelView ? 'channels.title' : 'dashboard.title')} currentUser={session.user} onLogout={logout}>
          {channelView ? <ChannelPage /> : <DashboardPage />}
        </AppShell>
      )}
    </SessionBoundary>
  )
}
