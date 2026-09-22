import { useMutation } from '@tanstack/react-query'
import { ArrowLeft, ArrowRight, Building2, LoaderCircle } from 'lucide-react'
import { type FormEvent, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Button, Input } from '@heroui/react'

import { cn } from '@/lib/utils'
import {
  buttonLight,
  buttonPrimaryLg,
  inputTextClass,
  inputWrapperClass,
} from '@/features/auth/auth-form-styles'
import {
  discoverOrganizationSso,
  startPublicOrganizationSsoLogin,
} from '@/features/auth/organization-sso-discovery-api'
import { submitOrganizationSsoSamlPostAction } from '@/features/organization-sso/organization-sso-authn-request'

type OrganizationSsoStatus = 'idle' | 'available' | 'unavailable' | 'error'

/**
 * 企业登录表单：由登录卡片底部的企业图标切换进入。
 * 与账号密码表单分开，两套流程各自持有输入与状态，互不覆盖对方的错误提示。
 */
export function OrganizationSsoLoginForm({ onBack }: { onBack: () => void }) {
  const { t } = useTranslation()
  const [identifier, setIdentifier] = useState('')
  const [status, setStatus] = useState<OrganizationSsoStatus>('idle')
  const [starting, setStarting] = useState(false)
  const identifierRef = useRef('')
  const discoveryMutation = useMutation({ mutationFn: discoverOrganizationSso })
  const busy = discoveryMutation.isPending || starting

  async function discover() {
    const value = identifier.trim()
    if (!value) {
      setStatus('error')
      return
    }
    setStatus('idle')
    try {
      const response = await discoveryMutation.mutateAsync(value)
      // 输入已经改过就不要用旧请求的结果覆盖当前状态。
      if (identifierRef.current.trim() === value) {
        setStatus(response?.available ? 'available' : 'unavailable')
      }
    } catch {
      if (identifierRef.current.trim() === value) {
        setStatus('error')
      }
    }
  }

  async function start() {
    const value = identifier.trim()
    if (!value || status !== 'available') return
    setStarting(true)
    try {
      const response = await startPublicOrganizationSsoLogin(value)
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
      setStatus('error')
    } finally {
      setStarting(false)
    }
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    void (status === 'available' ? start() : discover())
  }

  return (
    <form className="animate-in fade-in-0 grid gap-5 duration-200" onSubmit={handleSubmit}>
      <Button
        className={cn(buttonLight, '-ml-2 h-8 px-2 text-xs')}
        type="button"
        variant="flat"
        onPress={onBack}
      >
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
          classNames={{ inputWrapper: cn('h-11 px-3', inputWrapperClass), input: inputTextClass }}
          placeholder={t('auth.login.organizationSso.placeholder')}
          value={identifier}
          isDisabled={busy}
          aria-describedby={status !== 'idle' ? 'organization-sso-status' : undefined}
          onChange={(event) => {
            identifierRef.current = event.target.value
            setIdentifier(event.target.value)
            setStatus('idle')
          }}
        />
      </div>

      <Button
        data-slot="button"
        type="submit"
        size="lg"
        color="primary"
        className={cn(buttonPrimaryLg, 'w-full')}
        isDisabled={busy}
      >
        {busy
          ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
          : status === 'available'
            ? <ArrowRight className="size-4" aria-hidden="true" />
            : <Building2 className="size-4" aria-hidden="true" />}
        {starting
          ? t('auth.login.organizationSso.starting')
          : discoveryMutation.isPending
            ? t('auth.login.organizationSso.checking')
            : status === 'available'
              ? t('auth.login.organizationSso.continue')
              : t('auth.login.organizationSso.check')}
      </Button>

      {status !== 'idle' ? (
        <p
          id="organization-sso-status"
          role={status === 'error' ? 'alert' : 'status'}
          className={status === 'available'
            ? 'text-sm leading-5 text-success'
            : status === 'error'
              ? 'text-sm leading-5 text-destructive'
              : 'text-sm leading-5 text-muted-foreground'}
        >
          {t(`auth.login.organizationSso.${status}`)}
        </p>
      ) : null}

      <p className="text-center text-sm text-muted-foreground">
        {t('auth.login.organizationSso.noOrganization')}{' '}
        <a href="/register" className="font-medium text-info underline-offset-4 hover:underline">
          {t('auth.login.organizationSso.createAccount')}
        </a>
      </p>
    </form>
  )
}
