import { Button, Chip, Select, SelectItem, Skeleton } from '@heroui/react'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

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
  // HeroUI Select 的动态选项必须走 items + 渲染函数（数组子节点不被类型接受）。
  const providerItems = controller.providers.map((item) => ({ key: item.provider, label: t(`credentials.oauth.providers.${item.provider}`) }))
  return (
    <div className="mt-3 grid gap-1.5">
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={fieldId}>{t('credentials.oauth.provider')}</label>
      {controller.providersQuery.isPending ? (
        <Skeleton className="h-8 rounded-lg" aria-label={t('credentials.oauth.providersLoading')} />
      ) : controller.providersQuery.isError ? (
        <CredentialOAuthNotice tone="error" message={t('credentials.oauth.providersFailed')}>
          <Button isIconOnly aria-label={t('credentials.actions.retry')} className="size-6 min-w-6" size="sm" type="button" variant="light" onClick={() => controller.retryProviders()}><RefreshCw className="size-3" aria-hidden="true" /></Button>
        </CredentialOAuthNotice>
      ) : controller.providers.length === 0 ? (
        <CredentialOAuthNotice tone="neutral" message={t('credentials.oauth.providersEmpty')} />
      ) : controller.unsupportedBoundProvider ? (
        <CredentialOAuthNotice tone="error" message={t('credentials.oauth.providerUnsupported', { provider: credential.oauth_provider })} />
      ) : controller.boundProvider !== undefined && controller.selectedStatus === undefined ? (
        <CredentialOAuthNotice tone="error" message={t('credentials.oauth.providerNotConfigured')}>
          <Button isIconOnly aria-label={t('credentials.actions.retry')} className="size-6 min-w-6" size="sm" type="button" variant="light" onClick={() => controller.retryProviders()}><RefreshCw className="size-3" aria-hidden="true" /></Button>
        </CredentialOAuthNotice>
      ) : controller.boundProvider !== undefined ? (
        <div className="flex h-8 items-center gap-2 rounded-lg border border-input bg-background px-2.5">
          <Chip size="sm" variant="flat">{t(`credentials.oauth.providers.${controller.boundProvider}`)}</Chip>
          <span className="truncate text-[0.6875rem] text-muted-foreground">{t('credentials.oauth.providerBound')}</span>
        </div>
      ) : (
        <Select
          aria-label={t('credentials.oauth.provider')}
          id={fieldId}
          isDisabled={controller.status === 'waiting'}
          items={providerItems}
          selectedKeys={controller.provider ? [controller.provider] : []}
          size="sm"
          onSelectionChange={(keys) => {
            const next = Array.from(keys)[0]
            if (next !== undefined) controller.selectProvider(next as AdminOAuthProvider)
          }}
        >
          {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
        </Select>
      )}
    </div>
  )
}
