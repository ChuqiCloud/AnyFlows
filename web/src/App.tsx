import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { routeFromHash, type AppRoute, type AppView } from '@/app-route'
import { AppShell } from '@/components/layout/app-shell'
import { AuthenticationSettingsPage } from '@/features/authentication-settings/authentication-settings-page'
import { ForgotPasswordPage } from '@/features/auth/forgot-password-page'
import { LoginPage, type LoginNotice } from '@/features/auth/login-page'
import { ResetPasswordPage } from '@/features/auth/reset-password-page'
import { SessionBoundary } from '@/features/auth/session-boundary'
import { endManagementSession } from '@/features/auth/session-query'
import { ApiKeyPage } from '@/features/api-keys/api-key-page'
import { ChannelPage } from '@/features/channels/channel-page'
import { DashboardPage } from '@/features/dashboard/dashboard-page'
import { EmailSettingsPage } from '@/features/email-settings/email-settings-page'
import { LandingPage } from '@/features/landing/landing-page'
import { ModelPage } from '@/features/models/model-page'
import { PublicModelPage } from '@/features/models/public-model-page'
import { PlaygroundPage } from '@/features/playground/playground-page'
import { PublicPlaygroundSharePage } from '@/features/playground/public-playground-share-page'
import { ProfilePage } from '@/features/profile/profile-page'
import { RegistrationPage } from '@/features/registration/registration-page'
import { SiteSettingsPage } from '@/features/site-settings/site-settings-page'
import { TokenPage } from '@/features/tokens/token-page'
import { UsageLogPage } from '@/features/usage-logs/usage-log-page'
import { UserPage } from '@/features/users/user-page'
import { SetupGateLoading, SetupGateUnavailable } from '@/features/setup/setup-gate-state'
import { SetupPage } from '@/features/setup/setup-page'
import { useInitialSetupStatus } from '@/features/setup/setup-query'

// 临时的 hash 视图切换：TanStack Router 接入前，先让公开页与控制台都可访达。
// 路由落地后此处整体由文件路由替换。
function useHashRoute(): [AppRoute, (hash: string) => void] {
  const [hash, setHash] = useState(() => window.location.hash)

  useEffect(() => {
    const onHashChange = () => setHash(window.location.hash)

    window.addEventListener('hashchange', onHashChange)
    return () => window.removeEventListener('hashchange', onHashChange)
  }, [])

  const navigate = useCallback((nextHash: string) => {
    // 同步更新 React 路由状态，避免 setup 缓存切换与原生 hashchange 之间出现竞态。
    setHash(nextHash)
    window.location.hash = nextHash
  }, [])

  return [routeFromHash(hash), navigate]
}

type ManagementEntryProps = {
  view: Exclude<AppView, 'landing' | 'public-models' | 'register' | 'share' | 'forgot-password' | 'reset-password'>
  loginNotice?: LoginNotice
  onAuthenticated: () => void
  onLoginRequired: (reason: 'required' | 'sessionExpired' | 'setupComplete') => void
  onUserHomeRequired: () => void
  onPasswordChanged: () => void
  onLogout: () => void
}

/** 在登录或控制台挂载前完成首次安装状态判定。 */
function ManagementEntry({
  view,
  loginNotice,
  onAuthenticated,
  onLoginRequired,
  onUserHomeRequired,
  onPasswordChanged,
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
    : view === 'site-settings'
      ? 'siteSettings.title'
      : view === 'authentication-settings'
        ? 'authenticationSettings.title'
        : view === 'email-settings'
          ? 'emailSettings.title'
          : view === 'users'
            ? 'users.title'
            : view === 'api-keys'
              ? 'apiKeys.title'
              : view === 'models'
                ? 'models.title'
                : view === 'playground'
                  ? 'playground.title'
                  : view === 'profile'
                    ? 'profile.title'
                    : view === 'tokens'
                      ? 'tokens.title'
                      : view === 'usage-logs'
                        ? 'usageLogs.title'
                        : 'dashboard.title'
  const content = view === 'channels'
    ? <ChannelPage />
    : view === 'site-settings'
      ? <SiteSettingsPage />
      : view === 'authentication-settings'
        ? <AuthenticationSettingsPage />
        : view === 'email-settings'
          ? <EmailSettingsPage />
          : view === 'users'
            ? <UserPage />
            : view === 'api-keys'
              ? <ApiKeyPage />
              : view === 'models'
                ? <ModelPage authenticated />
                : view === 'playground'
                  ? <PlaygroundPage />
                  : view === 'profile'
                    ? <ProfilePage onPasswordChanged={onPasswordChanged} />
                    : view === 'tokens'
                      ? <TokenPage />
                      : view === 'usage-logs'
                        ? <UsageLogPage />
                        : <DashboardPage />

  const requireAdmin = view !== 'api-keys' && view !== 'models' && view !== 'playground' && view !== 'profile'

  return (
    <SessionBoundary
      requireAdmin={requireAdmin}
      onSessionEnded={onLoginRequired}
      onUserHomeRequired={onUserHomeRequired}
    >
      {(session) => (
        <AppShell title={t(titleKey)} currentUser={session.user} onLogout={onLogout}>
          {content}
        </AppShell>
      )}
    </SessionBoundary>
  )
}

export default function App() {
  const [route, navigate] = useHashRoute()
  const [loginNotice, setLoginNotice] = useState<LoginNotice>()
  const { view } = route

  const openLogin = useCallback((reason: 'required' | 'sessionExpired' | 'setupComplete') => {
    setLoginNotice(reason === 'required' ? undefined : reason)
    navigate('#/login')
  }, [navigate])

  const openConsole = useCallback(() => {
    setLoginNotice(undefined)
    navigate('#/console')
  }, [navigate])

  const openUserHome = useCallback(() => {
    setLoginNotice(undefined)
    navigate('#/console/models')
  }, [navigate])

  const openPasswordResetSuccess = useCallback(() => {
    setLoginNotice('passwordResetSuccess')
    navigate('#/login')
  }, [navigate])

  const openPasswordChanged = useCallback(() => {
    // 密码修改会递增会话版本，先清理本地旧令牌，再回到登录入口。
    endManagementSession()
    setLoginNotice('passwordChanged')
    navigate('#/login')
  }, [navigate])

  const logout = useCallback(() => {
    endManagementSession()
    setLoginNotice(undefined)
    navigate('#/login')
  }, [navigate])

  if (view === 'landing') {
    return <LandingPage />
  }
  if (view === 'public-models') {
    return <PublicModelPage />
  }
  if (view === 'register') {
    return <RegistrationPage onAuthenticated={openUserHome} />
  }
  if (view === 'forgot-password') {
    return <ForgotPasswordPage />
  }
  if (view === 'reset-password') {
    return <ResetPasswordPage token={route.resetToken} onCompleted={openPasswordResetSuccess} />
  }
  if (view === 'share') {
    return <PublicPlaygroundSharePage key={route.shareToken ?? 'invalid'} token={route.shareToken} />
  }

  return (
    <ManagementEntry
      view={view}
      loginNotice={loginNotice}
      onAuthenticated={openConsole}
      onLoginRequired={openLogin}
      onUserHomeRequired={openUserHome}
      onPasswordChanged={openPasswordChanged}
      onLogout={logout}
    />
  )
}
