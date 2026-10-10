import { Suspense } from 'react'
import { useTranslation } from 'react-i18next'

import { supportsExtensionRoute } from './model'
import { useConsoleExtensionCatalog } from './query'
import { findConsoleExtensionRoute } from './registry'

export function ConsoleExtensionPage({ path }: { path: string }) {
  const { t } = useTranslation()
  const catalog = useConsoleExtensionCatalog()
  const route = findConsoleExtensionRoute(path)
  const loading = <p role="status" className="py-6 text-sm text-muted-foreground">{t('auth.session.loading')}</p>
  if (!route) return <p role="alert" className="py-6 text-sm text-muted-foreground">{t('auth.session.unavailableTitle')}</p>
  if (catalog.isPending) return loading
  if (catalog.isError) {
    return <div role="alert" className="grid gap-3 py-6 text-sm">
      <p>{t('auth.session.unavailableTitle')}</p>
      <button className="w-fit text-brand underline" type="button" onClick={() => void catalog.refetch()}>{t('auth.session.retry')}</button>
    </div>
  }
  if (!supportsExtensionRoute(route, catalog.data)) {
    return <p role="alert" className="py-6 text-sm text-muted-foreground">{t('auth.session.unavailableTitle')}</p>
  }
  const { Component } = route
  return <Suspense fallback={loading}><Component /></Suspense>
}
