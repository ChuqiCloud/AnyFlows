import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import type { AdminCredential, AdminOAuthProvider } from '@/lib/api/generated/types.gen'
import type { CredentialOAuthController } from './credential-oauth-controller'
import { CredentialOAuthNotice } from './credential-oauth-notice'

/** 展示 Provider 配置状态；已绑定凭据不可在前端改绑。 */
export function CredentialOAuthProviderField({ controller, credential }: {
  controller: CredentialOAuthController
  credential: AdminCredential
}) {
  const { t } = useTranslation()
  const fieldId = `credential-${credential.id}-oauth-provider`
  return (
    <div className="mt-3 grid gap-1.5">
      <Label htmlFor={fieldId}>{t('credentials.oauth.provider')}</Label>
      {controller.providersQuery.isPending ? (
        <Skeleton className="h-8 rounded-lg" aria-label={t('credentials.oauth.providersLoading')} />
      ) : controller.providersQuery.isError ? (
        <CredentialOAuthNotice tone="error" message={t('credentials.oauth.providersFailed')}>
          <Button type="button" size="icon-xs" variant="ghost" aria-label={t('credentials.actions.retry')} onClick={() => controller.retryProviders()}><RefreshCw aria-hidden="true" /></Button>
        </CredentialOAuthNotice>
      ) : controller.providers.length === 0 ? (
        <CredentialOAuthNotice tone="neutral" message={t('credentials.oauth.providersEmpty')} />
      ) : controller.unsupportedBoundProvider ? (
        <CredentialOAuthNotice tone="error" message={t('credentials.oauth.providerUnsupported', { provider: credential.oauth_provider })} />
      ) : controller.boundProvider !== undefined && controller.selectedStatus === undefined ? (
        <CredentialOAuthNotice tone="error" message={t('credentials.oauth.providerNotConfigured')}>
          <Button type="button" size="icon-xs" variant="ghost" aria-label={t('credentials.actions.retry')} onClick={() => controller.retryProviders()}><RefreshCw aria-hidden="true" /></Button>
        </CredentialOAuthNotice>
      ) : controller.boundProvider !== undefined ? (
        <div className="flex h-8 items-center gap-2 rounded-lg border border-input bg-background px-2.5">
          <Badge>{t(`credentials.oauth.providers.${controller.boundProvider}`)}</Badge>
          <span className="truncate text-[0.6875rem] text-muted-foreground">{t('credentials.oauth.providerBound')}</span>
        </div>
      ) : (
        <Select id={fieldId} value={controller.provider} disabled={controller.status === 'waiting'} onChange={(event) => controller.selectProvider(event.target.value as AdminOAuthProvider)}>
          {controller.providers.map((item) => <option key={item.provider} value={item.provider}>{t(`credentials.oauth.providers.${item.provider}`)}</option>)}
        </Select>
      )}
    </div>
  )
}
