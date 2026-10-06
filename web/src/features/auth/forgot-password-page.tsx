import { AlertCircle, ArrowLeft, CheckCircle2, KeyRound, LoaderCircle, Mail } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { useRequestPasswordReset } from '@/features/auth/password-reset-api'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { ApiError } from '@/lib/api'

type ForgotPasswordError =
  | 'emailNotConfigured'
  | 'emailDeliveryFailed'
  | 'invalidInput'
  | 'rateLimited'
  | 'unavailable'

/** 提交忘记密码请求；成功页不透露邮箱是否对应有效账户。 */
export function ForgotPasswordPage() {
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const mutation = useRequestPasswordReset()
  const [email, setEmail] = useState('')
  const [error, setError] = useState<ForgotPasswordError>()
  const [submitted, setSubmitted] = useState(false)
  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError(undefined)
    if (!isValidEmail(email)) {
      setError('invalidInput')
      return
    }
    try {
      await mutation.mutateAsync({ email })
      setSubmitted(true)
    } catch (requestError) {
      setError(classifyForgotPasswordError(requestError))
    }
  }

  return (
    <AuthPageShell site={site}>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_430px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('passwordReset.forgot.eyebrow', { siteName })}</p>
          <h1 className="max-w-[11em] text-[2.75rem] leading-[1.08] font-semibold">{t('passwordReset.forgot.heroTitle')}</h1>
          <p className="mt-5 max-w-[36ch] text-base leading-7 text-muted-foreground">{t('passwordReset.forgot.heroBody')}</p>
        </section>

        <Card elevation="overlay" className="mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl">
          <CardHeader className="gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
            <div className="mb-2 grid size-10 place-items-center rounded-xl border border-info/20 bg-info/10 text-info">
              {submitted ? <CheckCircle2 className="size-5" aria-hidden="true" /> : <KeyRound className="size-5" aria-hidden="true" />}
            </div>
            <CardTitle className="text-xl leading-tight">
              {t(submitted ? 'passwordReset.forgot.successTitle' : 'passwordReset.forgot.title')}
            </CardTitle>
            <CardDescription className="leading-6">
              {t(submitted ? 'passwordReset.forgot.successBody' : 'passwordReset.forgot.subtitle')}
            </CardDescription>
          </CardHeader>

          <CardContent className="px-6 pb-6 sm:px-7 sm:pb-7">
            {submitted ? (
              <div className="grid gap-4">
                <p className="rounded-lg border border-success/20 bg-success/8 px-3 py-2.5 text-sm leading-5 text-success">
                  {t('passwordReset.forgot.successHint')}
                </p>
                <Button className="w-full" asChild>
                  <a href="#/login"><ArrowLeft aria-hidden="true" />{t('passwordReset.actions.backToLogin')}</a>
                </Button>
              </div>
            ) : (
              <form className="grid gap-5" onSubmit={handleSubmit} noValidate>
                <div className="grid gap-2">
                  <label htmlFor="forgot-password-email" className="text-sm font-medium">{t('passwordReset.forgot.email')}</label>
                  <Input
                    id="forgot-password-email"
                    type="email"
                    name="email"
                    autoComplete="email"
                    autoFocus
                    required
                    maxLength={320}
                    className="h-11 px-3"
                    placeholder={t('passwordReset.forgot.emailPlaceholder')}
                    value={email}
                    aria-invalid={error ? true : undefined}
                    aria-describedby={error ? 'forgot-password-error' : 'forgot-password-help'}
                    onChange={(event) => {
                      setEmail(event.target.value)
                      setError(undefined)
                    }}
                  />
                  {error ? (
                    <p id="forgot-password-error" role="alert" className="flex gap-2 text-xs leading-5 text-destructive">
                      <AlertCircle className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
                      {t(`passwordReset.errors.${error}`)}
                    </p>
                  ) : (
                    <p id="forgot-password-help" className="text-[0.6875rem] leading-4 text-muted-foreground">
                      {t('passwordReset.forgot.emailHelp')}
                    </p>
                  )}
                </div>

                <Button type="submit" size="lg" className="w-full" disabled={mutation.isPending}>
                  {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Mail className="size-4" aria-hidden="true" />}
                  {t(mutation.isPending ? 'passwordReset.forgot.submitting' : 'passwordReset.forgot.submit')}
                </Button>
                <Button variant="ghost" className="w-full" asChild>
                  <a href="#/login"><ArrowLeft aria-hidden="true" />{t('passwordReset.actions.backToLogin')}</a>
                </Button>
              </form>
            )}
          </CardContent>
        </Card>
      </div>
    </AuthPageShell>
  )
}

function classifyForgotPasswordError(error: unknown): ForgotPasswordError {
  if (!(error instanceof ApiError)) return 'unavailable'
  const code = errorCode(error.details)
  if (code === 'email_not_configured') return 'emailNotConfigured'
  if (code === 'email_delivery_failed') return 'emailDeliveryFailed'
  if (error.status === 429 || code === 'password_reset_rate_limited') return 'rateLimited'
  if (error.status === 400) return 'invalidInput'
  return 'unavailable'
}

function isValidEmail(value: string) {
  const trimmed = value.trim()
  if (trimmed !== value || value.length === 0 || new TextEncoder().encode(value).length > 320) return false
  if ([...value].some((character) => /\s|\p{Cc}/u.test(character))) return false
  const parts = value.split('@')
  return parts.length === 2 && parts[0].length > 0 && parts[1].length > 0
}

function errorCode(details: unknown) {
  if (typeof details !== 'object' || details === null || !('code' in details)) return undefined
  return typeof details.code === 'string' ? details.code : undefined
}
