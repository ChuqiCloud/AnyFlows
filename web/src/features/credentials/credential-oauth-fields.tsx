import { useEffect, useMemo } from 'react'
import type { UseFormReturn } from 'react-hook-form'
import { RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
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
            <Button type="button" size="icon-xs" variant="ghost" onClick={() => query.refetch()}><RefreshCw aria-hidden="true" /></Button>
          </div>
        ) : providers.length === 0 && !unknownProvider ? (
          <div className="rounded-lg border border-[var(--hairline)] bg-surface-2 px-3 py-2 text-xs text-muted-foreground">{t('credentials.oauth.empty')}</div>
        ) : (
          <Select id="credential-oauth-provider" disabled={disabled} aria-invalid={Boolean(form.formState.errors.oauthProvider)} {...form.register('oauthProvider')}>
            {unknownProvider ? <option value={provider}>{provider}</option> : null}
            {providers.map((item) => <option key={item.provider} value={item.provider}>{t(`credentials.oauth.providers.${item.provider}`)}</option>)}
          </Select>
        )}
      </CredentialField>

      {selected ? (
        <div className="flex flex-wrap items-center gap-2">
          <Badge className={cn('border-transparent', selected.loopback_listener_ready ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning')}>
            {t(selected.loopback_listener_ready ? 'credentials.oauth.automaticCallback' : 'credentials.oauth.manualCallback')}
          </Badge>
          <span className="text-[0.6875rem] text-muted-foreground">{t('credentials.oauth.callbackPort', { port: selected.callback_port })}</span>
        </div>
      ) : null}

      <div className="grid gap-3 sm:grid-cols-2">
        <CredentialField id="credential-oauth-account" label={t('credentials.oauth.accountKey')} hint={t('credentials.oauth.optional')} error={form.formState.errors.oauthAccountKey?.message}>
          <Input id="credential-oauth-account" maxLength={255} disabled={disabled} {...form.register('oauthAccountKey')} />
        </CredentialField>
        <CredentialField id="credential-oauth-project" label={t('credentials.oauth.projectId')} hint={t('credentials.oauth.optional')} error={form.formState.errors.oauthProjectId?.message}>
          <Input id="credential-oauth-project" maxLength={255} disabled={disabled} {...form.register('oauthProjectId')} />
        </CredentialField>
      </div>
    </div>
  )
}
