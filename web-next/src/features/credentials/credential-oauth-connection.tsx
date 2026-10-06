import { Chip } from '@heroui/react'
import { Link2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { AdminCredential } from '@/lib/api/generated/types.gen'
import { cn } from '@/lib/utils'
import { useCredentialOAuthController } from './credential-oauth-controller'
import { CredentialOAuthProviderField } from './credential-oauth-provider-field'
import { CredentialOAuthStatusView } from './credential-oauth-status'

type CredentialOAuthConnectionProps = {
  channelType: string
  active: boolean
  channelId: number
  credential: AdminCredential
  onRefresh: () => Promise<unknown>
  onDelete: (credential: AdminCredential) => void
}

/** 组合 OAuth Provider 状态、授权控制器与操作视图。 */
export function CredentialOAuthConnection({ active, channelType, channelId, credential, onDelete, onRefresh }: CredentialOAuthConnectionProps) {
  const { t } = useTranslation()
  const controller = useCredentialOAuthController({
    active,
    channelType,
    channelId,
    credential,
    onRefresh,
    preparingLabel: t('credentials.oauth.preparing'),
  })

  return (
    <div className="mt-3 rounded-lg border border-[var(--hairline)] bg-surface-2 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2">
          <div className="grid size-7 shrink-0 place-items-center rounded-md bg-background text-muted-foreground"><Link2 className="size-3.5" aria-hidden="true" /></div>
          <div className="min-w-0">
            <p className="text-xs font-medium">{t('credentials.oauth.connectionTitle')}</p>
            <p className="mt-0.5 text-[0.6875rem] leading-4 text-muted-foreground">{t(credential.oauth_token_pending ? 'credentials.oauth.pendingConnectionHint' : 'credentials.oauth.connectionHint')}</p>
          </div>
        </div>
        {controller.selectedStatus ? (
          <Chip className={cn('shrink-0', controller.selectedStatus.loopback_listener_ready ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning')} size="sm" variant="flat">
            {t(controller.selectedStatus.loopback_listener_ready ? 'credentials.oauth.automaticCallback' : 'credentials.oauth.manualCallback')}
          </Chip>
        ) : null}
      </div>
      <CredentialOAuthProviderField controller={controller} credential={credential} />
      <CredentialOAuthStatusView controller={controller} credential={credential} onDelete={() => { controller.resetFlow(); onDelete(credential) }} />
    </div>
  )
}
