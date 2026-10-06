import { createContext, useCallback, useContext } from 'react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'

import { AppShell } from '@/components/layout/app-shell'
import { SessionBoundary } from '@/features/auth/session-boundary'
import { endManagementSession } from '@/features/auth/session-query'
import type { SessionResponse } from '@/lib/api/generated/types.gen'
import { requiresAdmin } from '@/routes/access'

/** 控制台页面共享当前会话，避免每个页面各自判定角色。 */
const ConsoleSessionContext = createContext<SessionResponse | null>(null)

export function useConsoleSession() {
  const session = useContext(ConsoleSessionContext)

  if (!session) {
    throw new Error('useConsoleSession 只能用于控制台路由内')
  }

  return session
}

/** 控制台外壳：先恢复会话，再挂载带侧边导航与管理边界的控制台布局。 */
export function ConsoleLayout() {
  const navigate = useNavigate()
  const { pathname } = useLocation()

  const openLogin = useCallback(
    (notice?: 'sessionExpired' | 'passwordChanged') => {
      navigate('/login', { replace: true, state: notice ? { notice } : undefined })
    },
    [navigate],
  )

  const handleSessionEnded = useCallback(
    (reason: 'required' | 'sessionExpired') => {
      openLogin(reason === 'required' ? undefined : 'sessionExpired')
    },
    [openLogin],
  )

  const handleUserHomeRequired = useCallback(() => {
    navigate('/console/models', { replace: true })
  }, [navigate])

  const handlePasswordChanged = useCallback(() => {
    // 密码修改会递增会话版本，先清理本地旧令牌，再回到登录入口。
    endManagementSession()
    openLogin('passwordChanged')
  }, [openLogin])

  const handleLogout = useCallback(() => {
    endManagementSession()
    openLogin()
  }, [openLogin])

  return (
    <SessionBoundary
      requireAdmin={requiresAdmin(pathname)}
      onSessionEnded={handleSessionEnded}
      onUserHomeRequired={handleUserHomeRequired}
    >
      {(session) => (
        <ConsoleSessionContext.Provider value={session}>
          <AppShell currentUser={session.user} onLogout={handleLogout}>
            <Outlet context={{ onPasswordChanged: handlePasswordChanged }} />
          </AppShell>
        </ConsoleSessionContext.Provider>
      )}
    </SessionBoundary>
  )
}
