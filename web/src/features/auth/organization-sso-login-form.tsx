import { useMutation } from '@tanstack/react-query'
import { ArrowLeft, ArrowRight, Building2, LoaderCircle } from 'lucide-react'
import { type FormEvent, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  discoverOrganizationSso,
  startPublicOrganizationSsoLogin,
} from '@/features/auth/organization-sso-discovery-api'
import { submitOrganizationSsoSamlPostAction } from '@/features/organization-sso/organization-sso-authn-request'

/** 企业 SSO 独立表单，避免触发账号密码表单的必填校验。 */
export function OrganizationSsoLoginForm({ onBack, registrationAvailable }: {
  onBack: () => void
  registrationAvailable: boolean
}) {
  const { t } = useTranslation()
  const [organizationIdentifier, setOrganizationIdentifier] = useState('')
  const organizationIdentifierRef = useRef('')
  const [organizationSsoStatus, setOrganizationSsoStatus] = useState<'idle' | 'available' | 'unavailable' | 'error'>('idle')
  const [organizationSsoStarting, setOrganizationSsoStarting] = useState(false)
  const organizationSsoMutation = useMutation({ mutationFn: discoverOrganizationSso })
  const busy = organizationSsoMutation.isPending || organizationSsoStarting

  async function handleOrganizationSsoDiscovery() {
    const identifier = organizationIdentifier.trim()
    if (!identifier) {
      setOrganizationSsoStatus('error')
      return
    }
    setOrganizationSsoStatus('idle')
    try {
      const response = await organizationSsoMutation.mutateAsync(identifier)
      if (organizationIdentifierRef.current.trim() === identifier) {
        setOrganizationSsoStatus(response?.available ? 'available' : 'unavailable')
      }
    } catch {
      if (organizationIdentifierRef.current.trim() === identifier) {
        setOrganizationSsoStatus('error')
      }
    }
  }

  async function handleOrganizationSsoStart() {
    const identifier = organizationIdentifier.trim()
    if (!identifier || organizationSsoStatus !== 'available') return
    setOrganizationSsoStarting(true)
    try {
      const response = await startPublicOrganizationSsoLogin(identifier)
      if (response.protocol === 'oidc') {
        window.location.assign(response.authorization_url)
        return
      }
      const submitted = submitOrganizationSsoSamlPostAction(response.action, [
        { name: 'SAMLRequest', value: response.saml_request },
        { name: 'RelayState', value: response.relay_state },
      ])
      if (!submitted) throw new Error('企业 SSO SAML 动作无效')
    } catch {
      setOrganizationSsoStatus('error')
    } finally {
      setOrganizationSsoStarting(false)
    }
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (busy) return
    void (organizationSsoStatus === 'available'
      ? handleOrganizationSsoStart()
      : handleOrganizationSsoDiscovery())
  }

  return (
    <form className="grid gap-5" onSubmit={handleSubmit}>
      <Button type="button" variant="ghost" size="sm" className="-ml-2 w-fit px-2" disabled={busy} onClick={onBack}>
        <ArrowLeft className="size-3.5" aria-hidden="true" />
        {t('auth.login.organizationSso.back')}
      </Button>

      <div className="grid gap-2">
        <label htmlFor="organization-sso-identifier" className="text-sm font-medium">
          {t('auth.login.organizationSso.identifier')}
        </label>
        <Input
          id="organization-sso-identifier"
          name="organization_sso_identifier"
          autoComplete="email"
          autoFocus
          required
          className="h-11 px-3"
          placeholder={t('auth.login.organizationSso.placeholder')}
          value={organizationIdentifier}
          disabled={busy}
          aria-describedby={organizationSsoStatus !== 'idle' ? 'organization-sso-status' : undefined}
          onChange={(event) => {
            organizationIdentifierRef.current = event.target.value
            setOrganizationIdentifier(event.target.value)
            setOrganizationSsoStatus('idle')
          }}
        />
      </div>

      <Button type="submit" size="lg" className="h-auto min-h-11 w-full whitespace-normal py-3" disabled={busy}>
        {busy
          ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
          : organizationSsoStatus === 'available'
            ? <ArrowRight className="size-4" aria-hidden="true" />
            : <Building2 className="size-4" aria-hidden="true" />}
        {organizationSsoStarting
          ? t('auth.login.organizationSso.starting')
          : organizationSsoMutation.isPending
            ? t('auth.login.organizationSso.checking')
            : organizationSsoStatus === 'available'
              ? t('auth.login.organizationSso.continue')
              : t('auth.login.organizationSso.check')}
      </Button>

      {organizationSsoStatus !== 'idle' ? (
        <p
          id="organization-sso-status"
          role={organizationSsoStatus === 'error' ? 'alert' : 'status'}
          className={organizationSsoStatus === 'available'
            ? 'text-sm leading-5 text-success'
            : organizationSsoStatus === 'error'
              ? 'text-sm leading-5 text-destructive'
              : 'text-sm leading-5 text-muted-foreground'}
        >
          {t(`auth.login.organizationSso.${organizationSsoStatus}`)}
        </p>
      ) : null}

      {registrationAvailable ? (
        <p className="text-center text-sm text-muted-foreground">
          {t('auth.login.organizationSso.noOrganization')}{' '}
          <a href="#/register" className="font-medium text-info underline-offset-4 hover:underline">
            {t('auth.login.organizationSso.createAccount')}
          </a>
        </p>
      ) : null}
    </form>
  )
}
