import { Button, Checkbox, Input, Select, SelectItem, Textarea } from '@heroui/react'
import { KeyRound, LoaderCircle, Plus, Save, ShieldCheck, X } from 'lucide-react'
import type { FormEvent, ReactNode } from 'react'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import type { OrganizationSsoProvider, OrganizationSsoProviderRequestWritable } from '@/lib/api/generated/types.gen'

import {
  organizationSsoErrorCode,
  useCreateOrganizationSsoProvider,
  useUpdateOrganizationSsoProvider,
} from './organization-sso-api'

type Protocol = 'oidc' | 'saml'
type ProviderStatus = 'draft' | 'active' | 'disabled'

const PROTOCOL_ITEMS: Protocol[] = ['oidc', 'saml']
const STATUS_ITEMS: ProviderStatus[] = ['draft', 'active', 'disabled']
const NAME_ID_FORMAT_ITEMS = ['persistent', 'transient', 'email_address'] as const

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
        <Button isIconOnly type="button" size="sm" title={t('organizationConsole.settings.sso.providers.cancel')} variant="light" onClick={onClose} isDisabled={busy}>
          <X className="size-3.5" aria-hidden="true" /><span className="sr-only">{t('organizationConsole.settings.sso.providers.cancel')}</span>
        </Button>
      </div>

      <form className="mt-4 grid gap-4" onSubmit={(event) => void submit(event)}>
        <div className="grid gap-3 sm:grid-cols-3">
          <Field label={t('organizationConsole.settings.sso.providers.displayName')} htmlFor="organization-sso-provider-name">
            <Input id="organization-sso-provider-name" isDisabled={busy} isRequired maxLength={128} size="sm" value={draft.displayName} onChange={(event) => update('displayName', event.target.value)} />
          </Field>
          <Field label={t('organizationConsole.settings.sso.providers.protocol')} htmlFor="organization-sso-provider-protocol">
            <Select
              aria-label={t('organizationConsole.settings.sso.providers.protocol')}
              id="organization-sso-provider-protocol"
              isDisabled={busy || provider !== undefined}
              selectedKeys={[draft.protocol]}
              size="sm"
              onSelectionChange={(keys) => {
                const next = Array.from(keys)[0]
                if (next === 'oidc' || next === 'saml') update('protocol', next)
              }}
            >
              {PROTOCOL_ITEMS.map((value) => <SelectItem key={value}>{t(`organizationConsole.settings.sso.providers.protocols.${value}`)}</SelectItem>)}
            </Select>
          </Field>
          <Field label={t('organizationConsole.settings.sso.providers.status')} htmlFor="organization-sso-provider-status">
            <Select
              aria-label={t('organizationConsole.settings.sso.providers.status')}
              id="organization-sso-provider-status"
              isDisabled={busy}
              selectedKeys={[draft.status]}
              size="sm"
              onSelectionChange={(keys) => {
                const next = Array.from(keys)[0]
                if (next === 'draft' || next === 'active' || next === 'disabled') update('status', next)
              }}
            >
              {STATUS_ITEMS.map((value) => <SelectItem key={value}>{t(`organizationConsole.settings.sso.providers.statuses.${value}`)}</SelectItem>)}
            </Select>
          </Field>
        </div>

        {draft.protocol === 'oidc' ? (
          <div className="grid gap-3 sm:grid-cols-2">
            <UrlField id="organization-sso-issuer" label={t('organizationConsole.settings.sso.providers.oidc.issuer')} value={draft.issuer} onChange={(value) => update('issuer', value)} disabled={busy} />
            <UrlField id="organization-sso-authorization-endpoint" label={t('organizationConsole.settings.sso.providers.oidc.authorizationEndpoint')} value={draft.authorizationEndpoint} onChange={(value) => update('authorizationEndpoint', value)} disabled={busy} />
            <UrlField id="organization-sso-token-endpoint" label={t('organizationConsole.settings.sso.providers.oidc.tokenEndpoint')} value={draft.tokenEndpoint} onChange={(value) => update('tokenEndpoint', value)} disabled={busy} />
            <UrlField id="organization-sso-jwks-uri" label={t('organizationConsole.settings.sso.providers.oidc.jwksUri')} value={draft.jwksUri} onChange={(value) => update('jwksUri', value)} disabled={busy} />
            <Field label={t('organizationConsole.settings.sso.providers.oidc.clientId')} htmlFor="organization-sso-client-id"><Input id="organization-sso-client-id" isDisabled={busy} isRequired maxLength={256} size="sm" value={draft.clientId} onChange={(event) => update('clientId', event.target.value)} /></Field>
            <Field label={t('organizationConsole.settings.sso.providers.oidc.audience')} htmlFor="organization-sso-audience"><Input id="organization-sso-audience" isDisabled={busy} isRequired maxLength={256} size="sm" value={draft.audience} onChange={(event) => update('audience', event.target.value)} /></Field>
            <div className="sm:col-span-2"><UrlField id="organization-sso-redirect-uri" label={t('organizationConsole.settings.sso.providers.oidc.redirectUri')} value={draft.redirectUri} onChange={(value) => update('redirectUri', value)} disabled={busy} hint={t('organizationConsole.settings.sso.providers.oidc.redirectHint')} /></div>
          </div>
        ) : (
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t('organizationConsole.settings.sso.providers.saml.idpEntityId')} htmlFor="organization-sso-idp-entity"><Input id="organization-sso-idp-entity" isDisabled={busy} isRequired size="sm" value={draft.idpEntityId} onChange={(event) => update('idpEntityId', event.target.value)} /></Field>
            <UrlField id="organization-sso-sso-url" label={t('organizationConsole.settings.sso.providers.saml.ssoUrl')} value={draft.ssoUrl} onChange={(value) => update('ssoUrl', value)} disabled={busy} />
            <Field label={t('organizationConsole.settings.sso.providers.saml.spEntityId')} htmlFor="organization-sso-sp-entity"><Input id="organization-sso-sp-entity" isDisabled={busy} isRequired size="sm" value={draft.spEntityId} onChange={(event) => update('spEntityId', event.target.value)} /></Field>
            <UrlField id="organization-sso-acs-url" label={t('organizationConsole.settings.sso.providers.saml.acsUrl')} value={draft.acsUrl} onChange={(value) => update('acsUrl', value)} disabled={busy} />
            <Field label={t('organizationConsole.settings.sso.providers.saml.nameIdFormat')} htmlFor="organization-sso-name-id">
              <Select
                aria-label={t('organizationConsole.settings.sso.providers.saml.nameIdFormat')}
                id="organization-sso-name-id"
                isDisabled={busy}
                selectedKeys={[draft.nameIdFormat]}
                size="sm"
                onSelectionChange={(keys) => {
                  const next = Array.from(keys)[0]
                  if (next === 'persistent' || next === 'transient' || next === 'email_address') update('nameIdFormat', next)
                }}
              >
                {NAME_ID_FORMAT_ITEMS.map((value) => <SelectItem key={value}>{value}</SelectItem>)}
              </Select>
            </Field>
            <Field label={t('organizationConsole.settings.sso.providers.saml.certificates')} htmlFor="organization-sso-certificates" hint={t('organizationConsole.settings.sso.providers.saml.certificatesHint')}><Textarea id="organization-sso-certificates" isDisabled={busy} isRequired minRows={5} size="sm" value={draft.trustedCertificates} onChange={(event) => update('trustedCertificates', event.target.value)} /></Field>
          </div>
        )}

        <div className="grid gap-3 border-t border-[var(--hairline)] pt-4 sm:grid-cols-2">
          <Field label={t('organizationConsole.settings.sso.providers.clientSecret')} htmlFor="organization-sso-client-secret" hint={t('organizationConsole.settings.sso.providers.clientSecretHint')}>
            <Input autoComplete="new-password" id="organization-sso-client-secret" isDisabled={busy} size="sm" type="password" value={draft.clientSecret} onChange={(event) => update('clientSecret', event.target.value)} />
          </Field>
          {provider ? <label className="flex items-center gap-2 self-end pb-2 text-xs"><Checkbox isDisabled={busy || Boolean(draft.clientSecret)} isSelected={draft.clearClientSecret} size="sm" onValueChange={(checked) => update('clearClientSecret', checked)} /><span>{t('organizationConsole.settings.sso.providers.clearClientSecret')}</span></label> : null}
        </div>
        {draft.status === 'active' ? <p className="flex items-start gap-2 rounded-lg bg-warning/10 px-3 py-2 text-xs leading-5 text-warning"><ShieldCheck className="mt-0.5 size-4 shrink-0" aria-hidden="true" />{t(`organizationConsole.settings.sso.providers.activeHint.${draft.protocol}`)}</p> : null}
        {errorCode ? <p role="alert" className="text-xs leading-5 text-destructive">{t(`organizationConsole.settings.sso.providers.errors.${errorCode === 'organization_sso_provider_conflict' ? 'conflict' : 'generic'}`)}</p> : mutation.isError ? <p role="alert" className="text-xs leading-5 text-destructive">{t('organizationConsole.settings.sso.providers.errors.generic')}</p> : null}
        <div className="flex flex-wrap justify-end gap-2">
          <Button type="button" size="sm" variant="light" onClick={onClose} isDisabled={busy}>{t('organizationConsole.settings.sso.providers.cancel')}</Button>
          <Button type="submit" color="primary" size="sm" isDisabled={!canSubmit}>{busy ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : provider ? <Save className="size-4" aria-hidden="true" /> : <Plus className="size-4" aria-hidden="true" />}{t(provider ? 'organizationConsole.settings.sso.providers.save' : 'organizationConsole.settings.sso.providers.create')}</Button>
        </div>
      </form>
    </section>
  )
}

function Field({ label, htmlFor, hint, children }: { label: string; htmlFor: string; hint?: string; children: ReactNode }) {
  // HeroUI 没有独立 Label 组件，表单标签保留原生元素以维持原有的标签/提示层级。
  return <div className="grid gap-1.5"><label className="text-xs font-medium leading-none text-foreground" htmlFor={htmlFor}>{label}</label>{children}{hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}</div>
}

function UrlField({ id, label, value, onChange, disabled, hint }: { id: string; label: string; value: string; onChange: (value: string) => void; disabled: boolean; hint?: string }) {
  return <Field label={label} htmlFor={id} hint={hint}><Input id={id} isDisabled={disabled} isRequired placeholder="https://" size="sm" type="url" value={value} onChange={(event) => onChange(event.target.value)} /></Field>
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
