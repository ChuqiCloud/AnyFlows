import { Check, Copy, Github, LoaderCircle, RefreshCw, ShieldAlert } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import {
  useAdminGitHubOAuthLoginSettings,
  useUpdateAdminGitHubOAuthLoginSettings,
} from '@/features/auth/oauth-login-api'

type SecretAction = 'keep' | 'replace' | 'clear'

/** 管理 GitHub OAuth App，密钥只支持保留、轮换和显式清除。 */
export function GitHubOAuthSettings() {
  const { t } = useTranslation()
  const settingsQuery = useAdminGitHubOAuthLoginSettings()
  const mutation = useUpdateAdminGitHubOAuthLoginSettings()
  const settings = settingsQuery.data
  const [enabled, setEnabled] = useState(false)
  const [clientId, setClientId] = useState('')
  const [secretAction, setSecretAction] = useState<SecretAction>('keep')
  const [clientSecret, setClientSecret] = useState('')
  const [validationError, setValidationError] = useState(false)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!settings) return
    setEnabled(settings.enabled)
    setClientId(settings.client_id ?? '')
    setSecretAction('keep')
    setClientSecret('')
    setValidationError(false)
  }, [settings])

  if (settingsQuery.isPending) {
    return <Skeleton className="h-64 w-full rounded-xl" />
  }
  if (settingsQuery.isError || !settings) {
    return (
      <div role="alert" className="rounded-xl border border-destructive/25 bg-destructive/8 p-4">
        <h3 className="text-sm font-semibold text-destructive">{t('authenticationSettings.oauth.errors.load')}</h3>
        <Button type="button" size="sm" variant="secondary" className="mt-3" onClick={() => void settingsQuery.refetch()}>
          <RefreshCw aria-hidden="true" />{t('authenticationSettings.actions.retry')}
        </Button>
      </div>
    )
  }

  const configuredSecret = secretAction === 'keep'
    ? settings.client_secret_configured
    : secretAction === 'replace'
      ? clientSecret.length > 0
      : false
  const configurationComplete = Boolean(settings.callback_url && clientId.trim() && configuredSecret)
  const dirty = enabled !== settings.enabled
    || clientId !== (settings.client_id ?? '')
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
        expected_version: settings.version,
        enabled,
        client_id: clientId.trim() || null,
        client_secret: secretAction === 'replace' ? clientSecret : null,
        clear_client_secret: secretAction === 'clear',
      })
    } catch {
      // 保留当前输入，密钥错误只使用稳定通用文案，不回显底层诊断。
    }
  }

  async function copyCallback() {
    if (!settings.callback_url) return
    try {
      await navigator.clipboard.writeText(settings.callback_url)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1_500)
    } catch {
      setCopied(false)
    }
  }

  return (
    <form onSubmit={(event) => void submit(event)} className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/45 shadow-[var(--shadow-subtle)]">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] px-4 py-3.5">
        <div className="flex items-center gap-2">
          <Github className="size-4" aria-hidden="true" />
          <h3 className="text-sm font-semibold">{t('authenticationSettings.oauth.title')}</h3>
          <Badge className={enabled ? 'border-success/25 bg-success/10 text-success' : undefined}>
            {t(enabled ? 'authenticationSettings.status.enabled' : 'authenticationSettings.status.disabled')}
          </Badge>
        </div>
        <Switch
          checked={enabled}
          disabled={mutation.isPending}
          aria-label={t('authenticationSettings.oauth.enabled')}
          onCheckedChange={(checked) => {
            setEnabled(checked)
            setValidationError(false)
          }}
        />
      </div>

      <div className="grid gap-5 px-4 py-5 md:grid-cols-2">
        <div className="grid content-start gap-1.5 md:col-span-2">
          <Label htmlFor="github-oauth-callback">{t('authenticationSettings.oauth.callback')}</Label>
          <div className="flex gap-2">
            <Input
              id="github-oauth-callback"
              value={settings.callback_url ?? t('authenticationSettings.oauth.callbackMissing')}
              readOnly
              className="font-mono text-xs"
            />
            <Button
              type="button"
              size="icon"
              variant="secondary"
              disabled={!settings.callback_url}
              aria-label={t('authenticationSettings.oauth.copyCallback')}
              title={t('authenticationSettings.oauth.copyCallback')}
              onClick={() => void copyCallback()}
            >
              {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
            </Button>
          </div>
          <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('authenticationSettings.oauth.callbackHint')}</p>
        </div>

        <div className="grid content-start gap-1.5">
          <Label htmlFor="github-oauth-client-id">{t('authenticationSettings.oauth.clientId')}</Label>
          <Input
            id="github-oauth-client-id"
            value={clientId}
            maxLength={255}
            autoComplete="off"
            onChange={(event) => {
              setClientId(event.target.value)
              setValidationError(false)
            }}
          />
        </div>

        <div className="grid content-start gap-1.5">
          <Label htmlFor="github-oauth-secret-action">{t('authenticationSettings.oauth.clientSecret')}</Label>
          <Select
            id="github-oauth-secret-action"
            value={secretAction}
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
            {t(settings.client_secret_configured
              ? 'authenticationSettings.oauth.secret.configured'
              : 'authenticationSettings.oauth.secret.missing')}
          </p>
        </div>

        {secretAction === 'replace' ? (
          <div className="grid content-start gap-1.5 md:col-span-2">
            <Label htmlFor="github-oauth-client-secret">{t('authenticationSettings.oauth.newSecret')}</Label>
            <Input
              id="github-oauth-client-secret"
              type="password"
              value={clientSecret}
              maxLength={4096}
              autoComplete="new-password"
              onChange={(event) => {
                setClientSecret(event.target.value)
                setValidationError(false)
              }}
            />
          </div>
        ) : null}
      </div>

      {!settings.callback_url || validationError ? (
        <div className="mx-4 mb-4 flex items-start gap-2.5 rounded-lg border border-warning/25 bg-warning/8 p-3 text-xs leading-5">
          <ShieldAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden="true" />
          <span>{t(!settings.callback_url
            ? 'authenticationSettings.oauth.errors.publicBaseUrl'
            : 'authenticationSettings.oauth.errors.incomplete')}</span>
        </div>
      ) : null}

      <div className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-4 py-3">
        <span className={mutation.isError ? 'text-xs text-destructive' : 'text-xs text-muted-foreground'} role={mutation.isError ? 'alert' : undefined}>
          {mutation.isError
            ? t('authenticationSettings.oauth.errors.save')
            : t('authenticationSettings.oauth.version', { version: settings.version })}
        </span>
        <Button type="submit" disabled={!dirty || mutation.isPending}>
          {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(mutation.isPending ? 'authenticationSettings.actions.saving' : 'authenticationSettings.actions.save')}
        </Button>
      </div>
    </form>
  )
}
