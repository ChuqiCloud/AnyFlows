import { Button } from '@heroui/react'
import { KeyRound, ShieldCheck } from 'lucide-react'
import { type KeyboardEvent } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useManagementSession } from '@/features/auth/session-query'

import { ApiKeyPage } from './api-key-page'
import { AccessTokenPage } from './access-token-page'

type CredentialTab = 'model' | 'system'

export function AccessCredentialsPage() {
  const { t } = useTranslation()
  const [searchParams, setSearchParams] = useSearchParams()
  const session = useManagementSession(true)
  const activeTab: CredentialTab = searchParams.get('tab') === 'system' ? 'system' : 'model'
  const selectTab = (tab: CredentialTab) => {
    const next = new URLSearchParams(searchParams)
    if (tab === 'system') next.set('tab', tab)
    else next.delete('tab')
    setSearchParams(next, { replace: true })
  }
  const moveTab = (event: KeyboardEvent) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
    event.preventDefault()
    const next = event.key === 'Home' ? 'model' : event.key === 'End' ? 'system' : activeTab === 'model' ? 'system' : 'model'
    selectTab(next)
    document.getElementById(`credentials-tab-${next}`)?.focus()
  }

  return (
    <div className="grid min-w-0 gap-4">
      <div role="tablist" aria-label={t('accessTokens.tabs.label')} className="flex w-fit max-w-full gap-1 border-b border-[var(--hairline)]" onKeyDown={moveTab}>
        <Button id="credentials-tab-model" type="button" role="tab" tabIndex={activeTab === 'model' ? 0 : -1} aria-selected={activeTab === 'model'} aria-controls="credentials-panel-model" size="sm" variant={activeTab === 'model' ? 'flat' : 'light'} className="rounded-b-none" onPress={() => selectTab('model')}>
          <KeyRound className="size-3.5" aria-hidden="true" />{t('accessTokens.tabs.modelKeys')}
        </Button>
        <Button id="credentials-tab-system" type="button" role="tab" tabIndex={activeTab === 'system' ? 0 : -1} aria-selected={activeTab === 'system'} aria-controls="credentials-panel-system" size="sm" variant={activeTab === 'system' ? 'flat' : 'light'} className="rounded-b-none" onPress={() => selectTab('system')}>
          <ShieldCheck className="size-3.5" aria-hidden="true" />{t('accessTokens.tabs.systemTokens')}
        </Button>
      </div>
      <section id={`credentials-panel-${activeTab}`} role="tabpanel" aria-labelledby={`credentials-tab-${activeTab}`} className="min-w-0">
        {activeTab === 'model' ? <ApiKeyPage /> : session.data ? <AccessTokenPage key={`${session.data.user.id}:${session.data.expires_at}`} /> : <p role="status" className="text-xs text-muted-foreground">{t('accessTokens.loading')}</p>}
      </section>
    </div>
  )
}
