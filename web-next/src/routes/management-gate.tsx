import { useEffect } from 'react'
import { Outlet, useLocation, useNavigate } from 'react-router-dom'

import { SetupGateLoading, SetupGateUnavailable } from '@/features/setup/setup-gate-state'
import { useInitialSetupStatus } from '@/features/setup/setup-query'

/** 首次安装前锁定管理入口；安装完成后 /setup 会自动回到登录页。 */
export function ManagementGate() {
  const setupQuery = useInitialSetupStatus()
  const navigate = useNavigate()
  const { pathname } = useLocation()
  const onSetupRoute = pathname === '/setup'

  useEffect(() => {
    if (setupQuery.data?.setup_required && !onSetupRoute) {
      navigate('/setup', { replace: true })
      return
    }

    if (setupQuery.data && !setupQuery.data.setup_required && onSetupRoute) {
      navigate('/login', { replace: true })
    }
  }, [navigate, onSetupRoute, setupQuery.data])

  if (setupQuery.isPending) {
    return <SetupGateLoading />
  }

  if (setupQuery.isError) {
    return <SetupGateUnavailable onRetry={() => void setupQuery.refetch()} />
  }

  if (setupQuery.data.setup_required && !onSetupRoute) {
    return <SetupGateLoading />
  }

  return <Outlet />
}
