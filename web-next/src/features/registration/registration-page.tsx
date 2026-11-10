import { Button } from '@heroui/react'
import { AlertCircle, ArrowRight, Boxes, Building2, RefreshCw, UserRoundX } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useEffect, useState } from 'react'

import { AuthCapabilityCard, AuthCapabilitySkeleton } from '@/features/auth/auth-capability-card'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { invitationAuthLink } from '@/features/auth/invitation-context'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { RegistrationForm } from './registration-form'

type RegistrationPageProps = {
  inviteCode?: string
  invitationInvalid?: boolean
  onAuthenticated: () => void
}

/** 根据公开站点能力呈现注册表单，并在未知状态下稳定失败关闭。 */
export function RegistrationPage({ inviteCode, invitationInvalid, onAuthenticated }: RegistrationPageProps) {
  const { t } = useTranslation()
  const [currentInviteCode, setCurrentInviteCode] = useState(inviteCode ?? '')
  const siteQuery = usePublicSiteSettings()
  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')
  const heroTitle = site?.brand.tagline ?? t('registration.hero.title')
  const heroBody = site?.brand.description ?? t('registration.hero.body')
  const passwordLoginEnabled = site?.authentication.password_login_enabled ?? false
  const registrationEnabled = site?.authentication.registration_enabled ?? false
  const oauthProviders = site?.authentication.oauth_providers ?? []
  const registrationAvailable = registrationEnabled && (passwordLoginEnabled || oauthProviders.length > 0)
  const registrationEmailRequired = site?.authentication.registration_email_required ?? false
  const loginHref = invitationAuthLink('/login', currentInviteCode)

  useEffect(() => {
    setCurrentInviteCode(inviteCode ?? '')
  }, [inviteCode])

  return (
    <AuthPageShell site={site}>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_430px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('registration.hero.eyebrow', { siteName })}</p>
          <h1 className="text-[2.75rem] leading-[1.08] font-semibold">{heroTitle}</h1>
          <p className="mt-5 max-w-[36ch] text-base leading-7 text-muted-foreground">{heroBody}</p>
        </section>

        {siteQuery.isPending ? (
          <AuthCapabilitySkeleton label={t('registration.status.loading')} />
        ) : siteQuery.isError ? (
          <AuthCapabilityCard
            icon={<AlertCircle className="size-5" aria-hidden="true" />}
            title={t('registration.status.unavailableTitle')}
            body={t('registration.status.unavailableBody')}
            action={(
              <Button type="button" color="primary" onClick={() => void siteQuery.refetch()}>
                <RefreshCw className="size-4" aria-hidden="true" />
                {t('registration.actions.retry')}
              </Button>
            )}
          />
        ) : invitationInvalid ? (
          <AuthCapabilityCard
            icon={<AlertCircle className="size-5" aria-hidden="true" />}
            title={t('registration.validation.inviteCode')}
            body={t('registration.errors.invitationRejected')}
            action={<Button as="a" color="primary" href="/register">{t('registration.actions.retry')}</Button>}
          />
        ) : registrationAvailable ? (
          <RegistrationForm
            emailRequired={registrationEmailRequired}
            turnstileSiteKey={site?.authentication.turnstile_site_key ?? null}
            initialInviteCode={inviteCode}
            passwordLoginEnabled={passwordLoginEnabled}
            oauthProviders={oauthProviders}
            onInviteCodeChange={setCurrentInviteCode}
            onAuthenticated={onAuthenticated}
          />
        ) : (
          <AuthCapabilityCard
            icon={<UserRoundX className="size-5" aria-hidden="true" />}
            title={t('registration.status.disabledTitle')}
            body={t('registration.status.disabledBody')}
            action={(
              <div className={passwordLoginEnabled || oauthProviders.length > 0 ? 'grid gap-2 sm:grid-cols-2' : 'grid gap-2'}>
                {passwordLoginEnabled || oauthProviders.length > 0 ? (
                  <Button as="a" color="primary" href={loginHref}>{t('registration.actions.login')}</Button>
                ) : null}
                <Button as="a" href="/models" variant="bordered">
                  <Boxes className="size-4" aria-hidden="true" />{t('registration.actions.models')}
                </Button>
              </div>
            )}
          />
        )}

        <section className="lg:col-span-2 border-t border-[var(--hairline)] pt-6" aria-labelledby="registration-enterprise-title">
          <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_auto] lg:items-center">
            <div className="min-w-0">
              <div className="flex items-center gap-2 text-sm font-semibold">
                <Building2 className="size-4 text-info" aria-hidden="true" />
                <h2 id="registration-enterprise-title">{t('registration.enterprise.title')}</h2>
              </div>
              <p className="mt-2 max-w-3xl text-xs leading-5 text-muted-foreground">{t('registration.enterprise.body')}</p>
              <ol className="mt-3 grid gap-2 text-xs text-muted-foreground sm:grid-cols-3">
                {(['account', 'apply', 'setup'] as const).map((step, index) => (
                  <li key={step} className="flex items-start gap-2">
                    <span className="grid size-5 shrink-0 place-items-center rounded-full border border-[var(--hairline)] font-mono text-[0.6875rem]">{index + 1}</span>
                    <span>{t(`registration.enterprise.steps.${step}`)}</span>
                  </li>
                ))}
              </ol>
            </div>
            <Button as="a" href={loginHref} variant="bordered">
              {t('registration.enterprise.login')}
              <ArrowRight className="size-4" aria-hidden="true" />
            </Button>
          </div>
        </section>
      </div>
    </AuthPageShell>
  )
}
