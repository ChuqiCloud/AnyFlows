import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'

import { QueryProvider } from '@/lib/query'
import { RouterNavigationBridge } from '@/lib/router-navigation'
import { Provider } from '@/provider'

import './index.css'
import './i18n'
import { App } from './App'

const rootElement = document.getElementById('root')

if (!rootElement) {
  throw new Error('AnyFlows root element is missing')
}

createRoot(rootElement).render(
  <StrictMode>
    <BrowserRouter>
      <Provider>
        <QueryProvider>
          <RouterNavigationBridge />
          <App />
        </QueryProvider>
      </Provider>
    </BrowserRouter>
  </StrictMode>,
)
