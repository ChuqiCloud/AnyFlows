import { Check, Copy, LoaderCircle, RefreshCw, ShieldAlert } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { ApiError } from '@/lib/api'
import {
  type BuiltinOAuthLoginProvider,
  useAdminOAuthLoginSettings,
  useUpdateAdminOAuthLoginSettings,
} from '@/features/auth/oauth-login-api'
import { OAuthProviderIcon } from '@/features/auth/oauth-provider-icon'

type SecretAction = 'keep' | 'replace' | 'clear'

type OAuthProviderSettingsProps = {
  provider: BuiltinOAuthLoginProvider
}

/** 管理内置 OAuth App，密钥只支持保留、轮换和显式清除。 */
export function OAuthProviderSettings({ provider }: OAuthProviderSettingsProps) {
  const { t } = useTranslation()
  const baseKey = `authenticationSettings.oauth.${provider}`
  const settingsQuery = useAdminOAuthLoginSettings(provider)
  const mutation = useUpdateAdminOAuthLoginSettings(provider)
  const settings = settingsQuery.data
  const hasProviderSpecificLabels = provider === 'wechat' || provider === 'telegram' || provider === 'google'
  const clientIdLabel = hasProviderSpecificLabels ? t(`${baseKey}.clientId`) : t('authenticationSettings.oauth.clientId')
  const clientSecretLabel = hasProviderSpecificLabels ? t(`${baseKey}.clientSecret`) : t('authenticationSettings.oauth.clientSecret')
  const newSecretLabel = hasProviderSpecificLabels ? t(`${baseKey}.newSecret`) : t('authenticationSettings.oauth.newSecret')
  const [enabled, setEnabled] = useState(false)
  const [clientId, setClientId] = useState('')
  const [issuerUrl, setIssuerUrl] = useState('')
  const [secretAction, setSecretAction] = useState<SecretAction>('keep')
  const [clientSecret, setClientSecret] = useState('')
  const [validationError, setValidationError] = useState(false)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!settings) return
    setEnabled(settings.enabled)
    setClientId(settings.client_id ?? '')
    setIssuerUrl(settings.issuer_url ?? '')
    setSecretAction('keep')
    setClientSecret('')
    setValidationError(false)
  }, [settings])

  if (settingsQuery.isPending) {
    return <Skeleton className="h-64 w-full rounded-xl" />
  }
  if (settingsQuery.isError || !settings) {
    const unsupported = settingsQuery.error instanceof ApiError && settingsQuery.error.status === 404
    return (
      <div role="alert" className={`rounded-xl border p-4 ${unsupported ? 'border-warning/25 bg-warning/8' : 'border-destructive/25 bg-destructive/8'}`}>
        <h3 className={`text-sm font-semibold ${unsupported ? 'text-warning' : 'text-destructive'}`}>
          {t(unsupported ? 'authenticationSettings.oauth.errors.unsupportedTitle' : `${baseKey}.errors.load`)}
        </h3>
        {unsupported ? (
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t('authenticationSettings.oauth.errors.unsupported', { provider: t(`${baseKey}.title`) })}
          </p>
        ) : null}
        <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void settingsQuery.refetch()}>
          <RefreshCw aria-hidden="true" />{t('authenticationSettings.actions.retry')}
        </Button>
      </div>
    )
  }
  const currentSettings = settings
  const configuredSecret = secretAction === 'keep'
    ? currentSettings.client_secret_configured
    : secretAction === 'replace'
      ? clientSecret.length > 0
      : false
  const configurationComplete = Boolean(currentSettings.callback_url && clientId.trim() && configuredSecret && (provider !== 'oidc' || issuerUrl.trim()))
  const dirty = enabled !== currentSettings.enabled
    || clientId !== (currentSettings.client_id ?? '')
    || issuerUrl !== (currentSettings.issuer_url ?? '')
    || secretAction !== 'keep'
    || clientSecret.length > 0
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (enabled && !configurationComplete) {
      setValidationError(true)
      return
    }
    setValidationError(false)
    try {
      await mutation.mutateAsync({
        expected_version: currentSettings.version,
        enabled,
        client_id: clientId.trim() || null,
        issuer_url: provider === 'oidc' ? issuerUrl.trim() || null : null,
        client_secret: secretAction === 'replace' ? clientSecret : null,
        clear_client_secret: secretAction === 'clear',
      })
    } catch {
      // 保留当前输入，密钥错误只使用稳定通用文案，不回显底层诊断。
    }
  }

  async function copyCallback() {
    if (!currentSettings.callback_url) return
    try {
      await navigator.clipboard.writeText(currentSettings.callback_url)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1_500)
    } catch {
      setCopied(false)
    }
  }

  return (
    <form
      aria-labelledby={`${provider}-oauth-title`}
      onSubmit={(event) => void submit(event)}
      className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/45 shadow-[var(--shadow-subtle)]"
    >
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] px-4 py-3.5">
        <div className="flex items-center gap-2">
          <OAuthProviderIcon provider={provider} />
          <h3 id={`${provider}-oauth-title`} className="text-sm font-semibold">{t(`${baseKey}.title`)}</h3>
          <Badge className={enabled ? 'border-success/25 bg-success/10 text-success' : undefined}>
            {t(enabled ? 'authenticationSettings.status.enabled' : 'authenticationSettings.status.disabled')}
          </Badge>
        </div>
        <Switch
          checked={enabled}
          disabled={mutation.isPending}
          aria-label={t(`${baseKey}.enabled`)}
          onCheckedChange={(checked) => {
            setEnabled(checked)
            setValidationError(false)
          }}
        />
      </div>

      <div className="grid gap-5 px-4 py-5 md:grid-cols-2">
        <div className="grid content-start gap-1.5 md:col-span-2">
          <Label htmlFor={`${provider}-oauth-callback`}>{t('authenticationSettings.oauth.callback')}</Label>
          <div className="flex gap-2">
            <Input
              id={`${provider}-oauth-callback`}
              value={currentSettings.callback_url ?? t('authenticationSettings.oauth.callbackMissing')}
              readOnly
              disabled={mutation.isPending}
              className="font-mono text-xs"
            />
            <Button
              type="button"
              size="icon"
              variant="secondary"
              disabled={!currentSettings.callback_url}
              aria-label={t('authenticationSettings.oauth.copyCallback')}
              title={t('authenticationSettings.oauth.copyCallback')}
              onClick={() => void copyCallback()}
            >
              {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
            </Button>
          </div>
          <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t(`${baseKey}.callbackHint`)}</p>
        </div>

        <div className="grid content-start gap-1.5">
          <Label htmlFor={`${provider}-oauth-client-id`}>{clientIdLabel}</Label>
          <Input
            id={`${provider}-oauth-client-id`}
            value={clientId}
            maxLength={255}
            autoComplete="off"
            disabled={mutation.isPending}
            onChange={(event) => {
              setClientId(event.target.value)
              setValidationError(false)
            }}
          />
        </div>

        {provider === 'oidc' ? (
          <div className="grid content-start gap-1.5 md:col-span-2">
            <Label htmlFor={`${provider}-oauth-issuer`}>{t('authenticationSettings.oauth.issuer')}</Label>
            <Input
              id={`${provider}-oauth-issuer`}
              type="url"
              value={issuerUrl}
              maxLength={2048}
              autoComplete="url"
              placeholder="https://id.example.com"
              disabled={mutation.isPending}
              onChange={(event) => {
                setIssuerUrl(event.target.value)
                setValidationError(false)
              }}
            />
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t(`${baseKey}.issuerHint`)}</p>
          </div>
        ) : provider === 'linuxdo' || provider === 'telegram' || provider === 'google' ? (
          <div className="grid content-start gap-1.5 md:col-span-2">
            <Label htmlFor={`${provider}-oauth-issuer`}>{t('authenticationSettings.oauth.issuer')}</Label>
            <Input
              id={`${provider}-oauth-issuer`}
              value={currentSettings.issuer_url ?? (provider === 'linuxdo' ? 'https://connect.linux.do' : provider === 'telegram' ? 'https://oauth.telegram.org' : 'https://accounts.google.com')}
              readOnly
              disabled={mutation.isPending}
              className="font-mono text-xs"
            />
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t(`${baseKey}.issuerHint`)}</p>
          </div>
        ) : null}

        <div className="grid content-start gap-1.5">
          <Label htmlFor={`${provider}-oauth-secret-action`}>{clientSecretLabel}</Label>
          <Select
            id={`${provider}-oauth-secret-action`}
            value={secretAction}
            disabled={mutation.isPending}
            onChange={(event) => {
              setSecretAction(event.target.value as SecretAction)
              setClientSecret('')
              setValidationError(false)
            }}
          >
            <option value="keep">{t('authenticationSettings.oauth.secret.keep')}</option>
            <option value="replace">{t('authenticationSettings.oauth.secret.replace')}</option>
            <option value="clear">{t('authenticationSettings.oauth.secret.clear')}</option>
          </Select>
          <p className="text-[0.6875rem] leading-4 text-muted-foreground">
            {t(currentSettings.client_secret_configured
              ? 'authenticationSettings.oauth.secret.configured'
              : 'authenticationSettings.oauth.secret.missing')}
          </p>
        </div>

        {secretAction === 'replace' ? (
          <div className="grid content-start gap-1.5 md:col-span-2">
            <Label htmlFor={`${provider}-oauth-client-secret`}>{newSecretLabel}</Label>
            <Input
              id={`${provider}-oauth-client-secret`}
              type="password"
              value={clientSecret}
              maxLength={4096}
              autoComplete="new-password"
              disabled={mutation.isPending}
              onChange={(event) => {
                setClientSecret(event.target.value)
                setValidationError(false)
              }}
            />
          </div>
        ) : null}
      </div>

      {!currentSettings.callback_url || validationError ? (
        <div className="mx-4 mb-4 flex items-start gap-2.5 rounded-lg border border-warning/25 bg-warning/8 p-3 text-xs leading-5">
          <ShieldAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden="true" />
          <span>{t(!currentSettings.callback_url
            ? `${baseKey}.errors.publicBaseUrl`
            : `${baseKey}.errors.incomplete`)}</span>
        </div>
      ) : null}

      <div className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-4 py-3">
        <span className={mutation.isError ? 'text-xs text-destructive' : 'text-xs text-muted-foreground'} role={mutation.isError ? 'alert' : undefined}>
          {mutation.isError
            ? t(`${baseKey}.errors.save`)
            : t(`${baseKey}.version`, { version: currentSettings.version })}
        </span>
        <Button type="submit" disabled={!dirty || mutation.isPending}>
          {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(mutation.isPending ? 'authenticationSettings.actions.saving' : 'authenticationSettings.actions.save')}
        </Button>
      </div>
    </form>
  )
}
