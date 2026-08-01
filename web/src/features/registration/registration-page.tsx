import { AlertCircle, Boxes, RefreshCw, UserRoundX } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { AuthCapabilityCard, AuthCapabilitySkeleton } from '@/features/auth/auth-capability-card'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { RegistrationForm } from './registration-form'

type RegistrationPageProps = {
  inviteCode?: string
  onAuthenticated: () => void
}

/** 根据公开站点能力呈现注册表单，并在未知状态下稳定失败关闭。 */
export function RegistrationPage({ inviteCode, onAuthenticated }: RegistrationPageProps) {
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')
  const heroTitle = site?.brand.tagline ?? t('registration.hero.title')
  const heroBody = site?.brand.description ?? t('registration.hero.body')
  const passwordLoginEnabled = site?.authentication.password_login_enabled ?? false
  const registrationEnabled = site?.authentication.registration_enabled ?? false
  const registrationAvailable = passwordLoginEnabled && registrationEnabled
  const registrationEmailRequired = site?.authentication.registration_email_required ?? false

  return (
    <AuthPageShell site={site}>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_430px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('registration.hero.eyebrow', { siteName })}</p>
          <h1 className="max-w-[11em] text-[2.75rem] leading-[1.08] font-semibold">{heroTitle}</h1>
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
              <Button type="button" onClick={() => void siteQuery.refetch()}>
                <RefreshCw aria-hidden="true" />
                {t('registration.actions.retry')}
              </Button>
            )}
          />
        ) : registrationAvailable ? (
          <RegistrationForm
            emailRequired={registrationEmailRequired}
            initialInviteCode={inviteCode}
            onAuthenticated={onAuthenticated}
          />
        ) : (
          <AuthCapabilityCard
            icon={<UserRoundX className="size-5" aria-hidden="true" />}
            title={t('registration.status.disabledTitle')}
            body={t('registration.status.disabledBody')}
            action={(
              <div className={passwordLoginEnabled ? 'grid gap-2 sm:grid-cols-2' : 'grid gap-2'}>
                {passwordLoginEnabled ? (
                  <Button asChild><a href="#/login">{t('registration.actions.login')}</a></Button>
                ) : null}
                <Button variant="secondary" asChild>
                  <a href="#/models"><Boxes aria-hidden="true" />{t('registration.actions.models')}</a>
                </Button>
              </div>
            )}
          />
        )}
      </div>
    </AuthPageShell>
  )
}
