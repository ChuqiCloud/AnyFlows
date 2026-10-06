import { AlertCircle, Boxes, RefreshCw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { Button, Card, CardBody, CardHeader } from '@heroui/react'

import { cn } from '@/lib/utils'
import { AccountLoginForm, type LoginNotice } from '@/features/auth/account-login-form'
import { AuthCapabilityCard, AuthCapabilitySkeleton } from '@/features/auth/auth-capability-card'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { buttonPrimary, buttonSecondary, cardClass } from '@/features/auth/auth-form-styles'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'

export type { LoginNotice }

type LoginPageProps = {
  notice?: LoginNotice
  onAuthenticated: () => void
}

/**
 * 登录页：公开站点能力就绪后挂载账号登录表单。
 */
export function LoginPage({ notice, onAuthenticated }: LoginPageProps) {
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()

  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')
  const heroTitle = site?.brand.tagline ?? t('auth.login.heroTitle')
  const heroBody = site?.brand.description ?? t('auth.login.heroBody')
  return (
    <AuthPageShell site={site}>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_400px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('auth.login.eyebrow', { siteName })}</p>
          <h1 className="text-[2.75rem] leading-[1.08] font-semibold">{heroTitle}</h1>
          <p className="mt-5 max-w-[36ch] text-base leading-7 text-muted-foreground">{heroBody}</p>
        </section>

        {siteQuery.isPending ? (
          <AuthCapabilitySkeleton label={t('auth.login.loadingCapabilities')} />
        ) : siteQuery.isError ? (
          <AuthCapabilityCard
            icon={<AlertCircle className="size-5" aria-hidden="true" />}
            title={t('auth.login.capabilityUnavailableTitle')}
            body={t('auth.login.capabilityUnavailableBody')}
            action={(
              <div className="grid gap-2 sm:grid-cols-2">
                {/* data-slot 让 index.css 的关闭动效规则继续命中按钮。 */}
                <Button data-slot="button" type="button" color="primary" className={buttonPrimary} onPress={() => void siteQuery.refetch()}>
                  <RefreshCw aria-hidden="true" />
                  {t('auth.login.retry')}
                </Button>
                <Button data-slot="button" as="a" href="/models" variant="flat" className={buttonSecondary}>
                  <Boxes aria-hidden="true" />{t('auth.login.browseModels')}
                </Button>
              </div>
            )}
          />
        ) : (
          <Card shadow="none" className={cn(cardClass, 'mx-auto w-full max-w-[400px] bg-card/94 backdrop-blur-2xl')}>
            {/* HeroUI 头部默认横排 + items-center + p-3，补回原竖排与 p-5 基准。 */}
            <CardHeader className="flex-col items-stretch gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
              <h3 data-slot="card-title" className="text-xl leading-tight font-semibold">
                {t('auth.login.title')}
              </h3>
              <p data-slot="card-description" className="text-sm text-muted-foreground leading-6">
                {t('auth.login.subtitle')}
              </p>
            </CardHeader>

            <CardBody className="p-5 pt-0 px-6 pb-6 sm:px-7 sm:pb-7">
              <AccountLoginForm
                notice={notice}
                site={site}
                onAuthenticated={onAuthenticated}
              />
            </CardBody>
          </Card>
        )}
      </div>
    </AuthPageShell>
  )
}
