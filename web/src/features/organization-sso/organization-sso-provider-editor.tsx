import { KeyRound, LoaderCircle, Plus, Save, ShieldCheck, X } from 'lucide-react'
import type { FormEvent, ReactNode } from 'react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'
import type { OrganizationSsoProvider, OrganizationSsoProviderRequestWritable } from '@/lib/api/generated/types.gen'

import {
  organizationSsoErrorCode,
  useCreateOrganizationSsoProvider,
  useUpdateOrganizationSsoProvider,
} from './organization-sso-api'

type Protocol = 'oidc' | 'saml'
type ProviderStatus = 'draft' | 'active' | 'disabled'

type ProviderDraft = {
  protocol: Protocol
  status: ProviderStatus
  displayName: string
  clientSecret: string
  clearClientSecret: boolean
  issuer: string
  authorizationEndpoint: string
  tokenEndpoint: string
  jwksUri: string
  clientId: string
  audience: string
  redirectUri: string
  idpEntityId: string
  ssoUrl: string
  spEntityId: string
  acsUrl: string
  nameIdFormat: 'persistent' | 'transient' | 'email_address'
  trustedCertificates: string
}

/** 使用字段化配置生成后端严格校验的 Provider 投影，避免管理员手写 JSON。 */
export function OrganizationSsoProviderEditor({
  organizationId,
  provider,
  onClose,
}: {
  organizationId: number
  provider?: OrganizationSsoProvider
  onClose: () => void
}) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState<ProviderDraft>(() => draftFromProvider(provider))
  const createMutation = useCreateOrganizationSsoProvider(organizationId)
  const updateMutation = useUpdateOrganizationSsoProvider(organizationId)
  const mutation = provider ? updateMutation : createMutation
  const busy = mutation.isPending
  const resetMutation = mutation.reset

  useEffect(() => {
    setDraft(draftFromProvider(provider))
    resetMutation()
  }, [provider, resetMutation])

  const metadata = useMemo(() => buildMetadata(draft), [draft])
  const valid = validateDraft(draft)
  const secretReady = draft.protocol !== 'oidc' || Boolean(provider?.secret_configured || draft.clientSecret.trim())
  const canSubmit = valid && secretReady && !busy

  const update = <K extends keyof ProviderDraft>(key: K, value: ProviderDraft[K]) => {
    setDraft((current) => ({ ...current, [key]: value }))
    mutation.reset()
  }

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!canSubmit) return
    const body: OrganizationSsoProviderRequestWritable = {
      expected_version: provider?.version ?? 0,
      protocol: draft.protocol,
      status: draft.status,
      display_name: draft.displayName.trim(),
      metadata_projection: metadata,
      client_secret: draft.clientSecret.trim() || undefined,
      clear_client_secret: draft.clearClientSecret,
    }
    try {
      if (provider) {
        await updateMutation.mutateAsync({ providerKey: provider.provider_key, body })
      } else {
        await createMutation.mutateAsync(body)
      }
      onClose()
    } catch {
      // 保留表单内容，让管理员可按服务端错误或冲突提示修正后重试。
    }
  }

  const errorCode = organizationSsoErrorCode(mutation.error)

  return (
    <section className="border-t border-[var(--hairline)] bg-surface-1/30 px-4 py-4" aria-labelledby="organization-sso-provider-editor-title">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand"><KeyRound className="size-4" aria-hidden="true" /></span>
          <div className="min-w-0">
            <h3 id="organization-sso-provider-editor-title" className="text-sm font-semibold">{t(provider ? 'organizationConsole.settings.sso.providers.editTitle' : 'organizationConsole.settings.sso.providers.createTitle')}</h3>
            <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('organizationConsole.settings.sso.providers.editorHint')}</p>
          </div>
        </div>
        <Button type="button" size="icon-sm" variant="ghost" title={t('organizationConsole.settings.sso.providers.cancel')} onClick={onClose} disabled={busy}>
          <X aria-hidden="true" /><span className="sr-only">{t('organizationConsole.settings.sso.providers.cancel')}</span>
        </Button>
      </div>

      <form className="mt-4 grid gap-4" onSubmit={(event) => void submit(event)}>
        <div className="grid gap-3 sm:grid-cols-3">
          <Field label={t('organizationConsole.settings.sso.providers.displayName')} htmlFor="organization-sso-provider-name">
            <Input id="organization-sso-provider-name" required maxLength={128} value={draft.displayName} disabled={busy} onChange={(event) => update('displayName', event.target.value)} />
          </Field>
          <Field label={t('organizationConsole.settings.sso.providers.protocol')} htmlFor="organization-sso-provider-protocol">
            <Select id="organization-sso-provider-protocol" value={draft.protocol} disabled={busy || provider !== undefined} onChange={(event) => update('protocol', event.target.value as Protocol)}>
              <option value="oidc">{t('organizationConsole.settings.sso.providers.protocols.oidc')}</option>
              <option value="saml">{t('organizationConsole.settings.sso.providers.protocols.saml')}</option>
            </Select>
          </Field>
          <Field label={t('organizationConsole.settings.sso.providers.status')} htmlFor="organization-sso-provider-status">
            <Select id="organization-sso-provider-status" value={draft.status} disabled={busy} onChange={(event) => update('status', event.target.value as ProviderStatus)}>
              {(['draft', 'active', 'disabled'] as const).map((status) => <option key={status} value={status}>{t(`organizationConsole.settings.sso.providers.statuses.${status}`)}</option>)}
            </Select>
          </Field>
        </div>

        {draft.protocol === 'oidc' ? (
          <div className="grid gap-3 sm:grid-cols-2">
            <UrlField id="organization-sso-issuer" label={t('organizationConsole.settings.sso.providers.oidc.issuer')} value={draft.issuer} onChange={(value) => update('issuer', value)} disabled={busy} />
            <UrlField id="organization-sso-authorization-endpoint" label={t('organizationConsole.settings.sso.providers.oidc.authorizationEndpoint')} value={draft.authorizationEndpoint} onChange={(value) => update('authorizationEndpoint', value)} disabled={busy} />
            <UrlField id="organization-sso-token-endpoint" label={t('organizationConsole.settings.sso.providers.oidc.tokenEndpoint')} value={draft.tokenEndpoint} onChange={(value) => update('tokenEndpoint', value)} disabled={busy} />
            <UrlField id="organization-sso-jwks-uri" label={t('organizationConsole.settings.sso.providers.oidc.jwksUri')} value={draft.jwksUri} onChange={(value) => update('jwksUri', value)} disabled={busy} />
            <Field label={t('organizationConsole.settings.sso.providers.oidc.clientId')} htmlFor="organization-sso-client-id"><Input id="organization-sso-client-id" required maxLength={256} value={draft.clientId} disabled={busy} onChange={(event) => update('clientId', event.target.value)} /></Field>
            <Field label={t('organizationConsole.settings.sso.providers.oidc.audience')} htmlFor="organization-sso-audience"><Input id="organization-sso-audience" required maxLength={256} value={draft.audience} disabled={busy} onChange={(event) => update('audience', event.target.value)} /></Field>
            <div className="sm:col-span-2"><UrlField id="organization-sso-redirect-uri" label={t('organizationConsole.settings.sso.providers.oidc.redirectUri')} value={draft.redirectUri} onChange={(value) => update('redirectUri', value)} disabled={busy} hint={t('organizationConsole.settings.sso.providers.oidc.redirectHint')} /></div>
          </div>
        ) : (
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t('organizationConsole.settings.sso.providers.saml.idpEntityId')} htmlFor="organization-sso-idp-entity"><Input id="organization-sso-idp-entity" required value={draft.idpEntityId} disabled={busy} onChange={(event) => update('idpEntityId', event.target.value)} /></Field>
            <UrlField id="organization-sso-sso-url" label={t('organizationConsole.settings.sso.providers.saml.ssoUrl')} value={draft.ssoUrl} onChange={(value) => update('ssoUrl', value)} disabled={busy} />
            <Field label={t('organizationConsole.settings.sso.providers.saml.spEntityId')} htmlFor="organization-sso-sp-entity"><Input id="organization-sso-sp-entity" required value={draft.spEntityId} disabled={busy} onChange={(event) => update('spEntityId', event.target.value)} /></Field>
            <UrlField id="organization-sso-acs-url" label={t('organizationConsole.settings.sso.providers.saml.acsUrl')} value={draft.acsUrl} onChange={(value) => update('acsUrl', value)} disabled={busy} />
            <Field label={t('organizationConsole.settings.sso.providers.saml.nameIdFormat')} htmlFor="organization-sso-name-id"><Select id="organization-sso-name-id" value={draft.nameIdFormat} disabled={busy} onChange={(event) => update('nameIdFormat', event.target.value as ProviderDraft['nameIdFormat'])}><option value="persistent">persistent</option><option value="transient">transient</option><option value="email_address">email_address</option></Select></Field>
            <Field label={t('organizationConsole.settings.sso.providers.saml.certificates')} htmlFor="organization-sso-certificates" hint={t('organizationConsole.settings.sso.providers.saml.certificatesHint')}><Textarea id="organization-sso-certificates" required value={draft.trustedCertificates} disabled={busy} rows={5} onChange={(event) => update('trustedCertificates', event.target.value)} /></Field>
          </div>
        )}

        <div className="grid gap-3 border-t border-[var(--hairline)] pt-4 sm:grid-cols-2">
          <Field label={t('organizationConsole.settings.sso.providers.clientSecret')} htmlFor="organization-sso-client-secret" hint={t('organizationConsole.settings.sso.providers.clientSecretHint')}>
            <Input id="organization-sso-client-secret" type="password" autoComplete="new-password" value={draft.clientSecret} disabled={busy} onChange={(event) => update('clientSecret', event.target.value)} />
          </Field>
          {provider ? <label className="flex items-center gap-2 self-end pb-2 text-xs"><Checkbox checked={draft.clearClientSecret} disabled={busy || Boolean(draft.clientSecret)} onCheckedChange={(checked) => update('clearClientSecret', checked === true)} /><span>{t('organizationConsole.settings.sso.providers.clearClientSecret')}</span></label> : null}
        </div>
        {draft.status === 'active' ? <p className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning"><ShieldCheck className="mt-0.5 size-4 shrink-0" aria-hidden="true" />{t(`organizationConsole.settings.sso.providers.activeHint.${draft.protocol}`)}</p> : null}
        {errorCode ? <p role="alert" className="text-xs leading-5 text-destructive">{t(`organizationConsole.settings.sso.providers.errors.${errorCode === 'organization_sso_provider_conflict' ? 'conflict' : 'generic'}`)}</p> : mutation.isError ? <p role="alert" className="text-xs leading-5 text-destructive">{t('organizationConsole.settings.sso.providers.errors.generic')}</p> : null}
        <div className="flex flex-wrap justify-end gap-2">
          <Button type="button" size="sm" variant="ghost" onClick={onClose} disabled={busy}>{t('organizationConsole.settings.sso.providers.cancel')}</Button>
          <Button type="submit" size="sm" disabled={!canSubmit}>{busy ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : provider ? <Save aria-hidden="true" /> : <Plus aria-hidden="true" />}{t(provider ? 'organizationConsole.settings.sso.providers.save' : 'organizationConsole.settings.sso.providers.create')}</Button>
        </div>
      </form>
    </section>
  )
}

function Field({ label, htmlFor, hint, children }: { label: string; htmlFor: string; hint?: string; children: ReactNode }) {
  return <div className="grid gap-1.5"><Label htmlFor={htmlFor}>{label}</Label>{children}{hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}</div>
}

function UrlField({ id, label, value, onChange, disabled, hint }: { id: string; label: string; value: string; onChange: (value: string) => void; disabled: boolean; hint?: string }) {
  return <Field label={label} htmlFor={id} hint={hint}><Input id={id} type="url" required placeholder="https://" value={value} disabled={disabled} onChange={(event) => onChange(event.target.value)} /></Field>
}

function draftFromProvider(provider?: OrganizationSsoProvider): ProviderDraft {
  const metadata = asRecord(provider?.metadata_projection)
  const protocol = provider?.protocol === 'saml' ? 'saml' : 'oidc'
  return {
    protocol,
    status: provider?.status === 'active' || provider?.status === 'disabled' ? provider.status : 'draft',
    displayName: provider?.display_name ?? '',
    clientSecret: '',
    clearClientSecret: false,
    issuer: readString(metadata.issuer),
    authorizationEndpoint: readString(metadata.authorization_endpoint),
    tokenEndpoint: readString(metadata.token_endpoint),
    jwksUri: readString(metadata.jwks_uri),
    clientId: readString(metadata.client_id),
    audience: readString(metadata.audience),
    redirectUri: readString(metadata.redirect_uri),
    idpEntityId: readString(metadata.idp_entity_id),
    ssoUrl: readString(metadata.sso_url),
    spEntityId: readString(metadata.sp_entity_id),
    acsUrl: readString(metadata.acs_url),
    nameIdFormat: metadata.name_id_format === 'transient' || metadata.name_id_format === 'email_address' ? metadata.name_id_format : 'persistent',
    trustedCertificates: Array.isArray(metadata.trusted_certificates) ? metadata.trusted_certificates.filter((value): value is string => typeof value === 'string').join('\n\n') : '',
  }
}

function buildMetadata(draft: ProviderDraft) {
  if (draft.protocol === 'oidc') return { issuer: draft.issuer.trim(), authorization_endpoint: draft.authorizationEndpoint.trim(), token_endpoint: draft.tokenEndpoint.trim(), jwks_uri: draft.jwksUri.trim(), client_id: draft.clientId.trim(), audience: draft.audience.trim(), redirect_uri: draft.redirectUri.trim(), protocol: 'oidc' }
  return { idp_entity_id: draft.idpEntityId.trim(), sso_url: draft.ssoUrl.trim(), sp_entity_id: draft.spEntityId.trim(), acs_url: draft.acsUrl.trim(), name_id_format: draft.nameIdFormat, trusted_certificates: splitCertificates(draft.trustedCertificates), protocol: 'saml' }
}

function validateDraft(draft: ProviderDraft) {
  if (!draft.displayName.trim()) return false
  const https = (value: string) => { try { return new URL(value).protocol === 'https:' } catch { return false } }
  if (draft.protocol === 'oidc') return [draft.issuer, draft.authorizationEndpoint, draft.tokenEndpoint, draft.jwksUri, draft.redirectUri].every(https) && Boolean(draft.clientId.trim() && draft.audience.trim())
  return Boolean(draft.idpEntityId.trim() && draft.spEntityId.trim() && https(draft.ssoUrl) && https(draft.acsUrl) && splitCertificates(draft.trustedCertificates).length > 0)
}

function splitCertificates(value: string) {
  return value.split(/(?=-----BEGIN CERTIFICATE-----)/g).map((item) => item.trim()).filter(Boolean)
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {}
}

function readString(value: unknown) {
  return typeof value === 'string' ? value : ''
}
