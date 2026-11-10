import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'

import { QueryProvider } from '@/lib/query'
import { RouterNavigationBridge } from '@/lib/router-navigation'
import { Provider } from '@/provider'
import { browserRouterBasename, migrateLegacyEpayHashReturn } from '@/features/wallet/payment-return-model'

import './index.css'
import './i18n'
import { App } from './App'

// Shared login callbacks carry one-time material in a fragment, including hash-based templates.
if (window.location.hash === '#/oauth/callback' || window.location.hash.startsWith('#/oauth/callback?')) {
  window.history.replaceState(null, '', `/oauth/callback${window.location.search}${window.location.hash}`)
} else {
  const migratedReturn = migrateLegacyEpayHashReturn(new URL(window.location.href))
  if (migratedReturn.href !== window.location.href) {
    window.history.replaceState(null, '', `${migratedReturn.pathname}${migratedReturn.search}`)
  }
}

const rootElement = document.getElementById('root')

if (!rootElement) {
  throw new Error('AnyFlows root element is missing')
}

createRoot(rootElement).render(
  <StrictMode>
    <BrowserRouter basename={browserRouterBasename(window.location.pathname) || undefined}>
      <Provider>
        <QueryProvider>
          <RouterNavigationBridge />
          <App />
        </QueryProvider>
      </Provider>
    </BrowserRouter>
  </StrictMode>,
)
