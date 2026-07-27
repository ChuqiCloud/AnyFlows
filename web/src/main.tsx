import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import './index.css'
import './i18n'
import App from './App.tsx'
import { QueryProvider } from '@/lib/query'
import { TooltipProvider } from '@/components/ui/tooltip'

const rootElement = document.getElementById('root')

if (!rootElement) {
  throw new Error('AnyFlows root element is missing')
}

createRoot(rootElement).render(
  <StrictMode>
    <QueryProvider>
      <TooltipProvider>
        <App />
      </TooltipProvider>
    </QueryProvider>
  </StrictMode>,
)
