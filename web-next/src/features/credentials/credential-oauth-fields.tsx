import { useEffect, useMemo } from 'react'
import type { UseFormReturn } from 'react-hook-form'
import { Button, Chip, Input, Select, SelectItem, Skeleton } from '@heroui/react'
import { Controller } from 'react-hook-form'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import { useAdminOAuthProviders } from './credential-api'
import { CredentialField } from './credential-form-field'
import type { CredentialFormValues } from './credential-form-model'

/** 只从启动配置目录选择 OAuth Provider，并结构化维护账号身份投影。 */
export function CredentialOAuthFields({ form, channelType, active, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  channelType: string
  active: boolean
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const provider = form.watch('oauthProvider')
  const fixedCodex = channelType === 'openai'
  const query = useAdminOAuthProviders(active)
  const providers = useMemo(() => query.data?.providers ?? [], [query.data])
  const selected = providers.find((item) => item.provider === provider)
  const unknownProvider = provider !== '' && selected === undefined

  useEffect(() => {
    if (fixedCodex) {
      if (provider !== 'codex') form.setValue('oauthProvider', 'codex', { shouldDirty: false, shouldValidate: true })
      return
    }
    if (!active || provider !== '' || providers.length === 0) return
    form.setValue('oauthProvider', providers[0].provider, { shouldDirty: true, shouldValidate: true })
  }, [active, fixedCodex, form, provider, providers])

  // HeroUI Select 的动态选项必须走 items + 渲染函数（数组子节点不被类型接受）。
  const providerItems = useMemo(() => {
    const known = providers.map((item) => ({ key: item.provider, label: t(`credentials.oauth.providers.${item.provider}`) }))
    return unknownProvider ? [{ key: provider, label: provider }, ...known] : known
  }, [provider, providers, t, unknownProvider])

  if (!active) return null
  return (
    <div className="grid gap-3">
      <CredentialField
        id="credential-oauth-provider"
        label={t('credentials.oauth.provider')}
        error={form.formState.errors.oauthProvider?.message}
      >
        {fixedCodex ? (
          <div className="flex h-9 items-center rounded-lg border border-[var(--hairline)] bg-surface-2 px-3 text-sm">OpenAI / Codex</div>
        ) : query.isPending ? (
          <Skeleton className="h-9 rounded-lg" aria-label={t('credentials.oauth.loading')} />
        ) : query.isError ? (
          <div role="alert" className="flex min-h-9 items-center justify-between gap-3 rounded-lg border border-destructive/25 bg-destructive/8 px-3 py-1.5">
            <span className="text-xs text-destructive">{t('credentials.oauth.loadFailed')}</span>
            <Button isIconOnly aria-label={t('credentials.actions.retry')} className="size-6 min-w-6" size="sm" type="button" variant="light" onClick={() => query.refetch()}><RefreshCw className="size-3" aria-hidden="true" /></Button>
          </div>
        ) : providers.length === 0 && !unknownProvider ? (
          <div className="rounded-lg border border-[var(--hairline)] bg-surface-2 px-3 py-2 text-xs text-muted-foreground">{t('credentials.oauth.empty')}</div>
        ) : (
          <Controller
            control={form.control}
            name="oauthProvider"
            render={({ field }) => (
              <Select
                aria-label={t('credentials.oauth.provider')}
                id="credential-oauth-provider"
                isDisabled={disabled}
                isInvalid={Boolean(form.formState.errors.oauthProvider)}
                items={providerItems}
                selectedKeys={field.value ? [field.value] : []}
                size="sm"
                onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? ''))}
              >
                {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
              </Select>
            )}
          />
        )}
      </CredentialField>

      {selected ? (
        <div className="flex flex-wrap items-center gap-2">
          <Chip className={cn(selected.loopback_listener_ready ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning')} size="sm" variant="flat">
            {t(selected.loopback_listener_ready ? 'credentials.oauth.automaticCallback' : 'credentials.oauth.manualCallback')}
          </Chip>
          <span className="text-[0.6875rem] text-muted-foreground">{t('credentials.oauth.callbackPort', { port: selected.callback_port })}</span>
        </div>
      ) : null}

      <div className="grid gap-3 sm:grid-cols-2">
        <CredentialField id="credential-oauth-account" label={t('credentials.oauth.accountKey')} hint={t('credentials.oauth.optional')} error={form.formState.errors.oauthAccountKey?.message}>
          <Input autoComplete="off" id="credential-oauth-account" isDisabled={disabled} maxLength={255} size="sm" {...form.register('oauthAccountKey')} />
        </CredentialField>
        <CredentialField id="credential-oauth-project" label={t('credentials.oauth.projectId')} hint={t('credentials.oauth.optional')} error={form.formState.errors.oauthProjectId?.message}>
          <Input autoComplete="off" id="credential-oauth-project" isDisabled={disabled} maxLength={255} size="sm" {...form.register('oauthProjectId')} />
        </CredentialField>
      </div>
    </div>
  )
}
