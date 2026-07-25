import { useTranslation } from 'react-i18next'

import { AppShell } from '@/components/layout/app-shell'

export default function App() {
  const { t } = useTranslation()

  return <AppShell title={t('dashboard.title')} />
}
