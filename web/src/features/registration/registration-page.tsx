import { AlertCircle, Boxes, RefreshCw, UserRoundX } from 'lucide-react'
import type { ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Skeleton } from '@/components/ui/skeleton'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { RegistrationForm } from './registration-form'
import { useRegistrationStatus } from './registration-api'

type RegistrationPageProps = {
  onAuthenticated: () => void
}

/** 根据公开能力状态呈现注册表单，并在未知状态下稳定失败关闭。 */
export function RegistrationPage({ onAuthenticated }: RegistrationPageProps) {
  const { t } = useTranslation()
  const statusQuery = useRegistrationStatus()

  return (
    <AuthPageShell>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_430px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('registration.hero.eyebrow')}</p>
          <h1 className="max-w-[11em] text-[2.75rem] leading-[1.08] font-semibold">
            {t('registration.hero.title')}
          </h1>
          <p className="mt-5 max-w-[36ch] text-base leading-7 text-muted-foreground">
            {t('registration.hero.body')}
          </p>
        </section>

        {statusQuery.isPending ? (
          <Card elevation="overlay" className="mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl" aria-label={t('registration.status.loading')}>
            <CardHeader className="gap-3 px-7 pt-7">
              <Skeleton className="size-10 rounded-xl" />
              <Skeleton className="h-6 w-40" />
              <Skeleton className="h-4 w-64 max-w-full" />
            </CardHeader>
            <CardContent className="grid gap-4 px-7 pb-7">
              {[0, 1, 2, 3].map((item) => <Skeleton key={item} className="h-11 rounded-lg" />)}
              <Skeleton className="h-11 rounded-lg" />
            </CardContent>
          </Card>
        ) : statusQuery.isError ? (
          <RegistrationStateCard
            icon={<AlertCircle className="size-5" aria-hidden="true" />}
            title={t('registration.status.unavailableTitle')}
            body={t('registration.status.unavailableBody')}
            action={(
              <Button type="button" onClick={() => void statusQuery.refetch()}>
                <RefreshCw aria-hidden="true" />
                {t('registration.actions.retry')}
              </Button>
            )}
          />
        ) : statusQuery.data.enabled ? (
          <RegistrationForm
            emailRequired={statusQuery.data.email_required}
            onAuthenticated={onAuthenticated}
          />
        ) : (
          <RegistrationStateCard
            icon={<UserRoundX className="size-5" aria-hidden="true" />}
            title={t('registration.status.disabledTitle')}
            body={t('registration.status.disabledBody')}
            action={(
              <div className="grid gap-2 sm:grid-cols-2">
                <Button asChild><a href="#/login">{t('registration.actions.login')}</a></Button>
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

type RegistrationStateCardProps = {
  icon: ReactNode
  title: string
  body: string
  action: ReactNode
}

function RegistrationStateCard({ icon, title, body, action }: RegistrationStateCardProps) {
  return (
    <Card elevation="overlay" className="mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl">
      <CardHeader className="gap-2 px-7 pt-7 pb-5">
        <div className="mb-2 grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-2 text-muted-foreground">
          {icon}
        </div>
        <CardTitle className="text-xl leading-tight">{title}</CardTitle>
        <CardDescription className="leading-6">{body}</CardDescription>
      </CardHeader>
      <CardContent className="px-7 pb-7">{action}</CardContent>
    </Card>
  )
}
