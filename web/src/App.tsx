import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AppShell } from '@/components/layout/app-shell'
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

  return hash.startsWith('#/console') ? 'console' : 'landing'
}

export default function App() {
  const { t } = useTranslation()
  const view = useHashView()

  if (view === 'landing') {
    return <LandingPage />
  }

  return (
    <AppShell title={t('dashboard.title')}>
      <DashboardPage />
    </AppShell>
  )
}
