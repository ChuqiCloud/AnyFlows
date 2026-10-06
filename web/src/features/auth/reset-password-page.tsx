import { AlertCircle, ArrowLeft, CheckCircle2, Eye, EyeOff, LoaderCircle, ShieldCheck } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { useConfirmPasswordReset } from '@/features/auth/password-reset-api'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { ApiError } from '@/lib/api'

type ResetPasswordError = 'invalidInput' | 'rejected' | 'unavailable'

type ResetPasswordPageProps = {
  token?: string
  onCompleted: () => void
}

/** 使用内存中的单次令牌提交新密码，并在挂载后立即清理地址栏查询串。 */
export function ResetPasswordPage({ token: initialToken, onCompleted }: ResetPasswordPageProps) {
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const mutation = useConfirmPasswordReset()
  const [token, setToken] = useState(initialToken)
  const [password, setPassword] = useState('')
  const [passwordConfirmation, setPasswordConfirmation] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [error, setError] = useState<ResetPasswordError>()
  const [completed, setCompleted] = useState(false)
  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')
  const tokenMissing = !token
  const passwordToggleLabel = showPassword
    ? t('passwordReset.reset.hidePassword')
    : t('passwordReset.reset.showPassword')

  useEffect(() => {
    if (window.location.hash.startsWith('#/reset-password')) {
      window.history.replaceState(null, document.title, `${window.location.pathname}${window.location.search}#/reset-password`)
    }
  }, [])

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError(undefined)
    if (!token || !isValidPassword(password) || password !== passwordConfirmation) {
      setError('invalidInput')
      return
    }
    try {
      await mutation.mutateAsync({ token, password })
      setCompleted(true)
      setToken(undefined)
      setPassword('')
      setPasswordConfirmation('')
    } catch (requestError) {
      setError(classifyResetPasswordError(requestError))
    }
  }

  return (
    <AuthPageShell site={site}>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_430px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('passwordReset.reset.eyebrow', { siteName })}</p>
          <h1 className="max-w-[11em] text-[2.75rem] leading-[1.08] font-semibold">{t('passwordReset.reset.heroTitle')}</h1>
          <p className="mt-5 max-w-[36ch] text-base leading-7 text-muted-foreground">{t('passwordReset.reset.heroBody')}</p>
        </section>

        <Card elevation="overlay" className="mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl">
          <CardHeader className="gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
            <div className="mb-2 grid size-10 place-items-center rounded-xl border border-info/20 bg-info/10 text-info">
              {completed ? <CheckCircle2 className="size-5" aria-hidden="true" /> : <ShieldCheck className="size-5" aria-hidden="true" />}
            </div>
            <CardTitle className="text-xl leading-tight">
              {t(completed ? 'passwordReset.reset.successTitle' : 'passwordReset.reset.title')}
            </CardTitle>
            <CardDescription className="leading-6">
              {t(completed ? 'passwordReset.reset.successBody' : 'passwordReset.reset.subtitle')}
            </CardDescription>
          </CardHeader>

          <CardContent className="px-6 pb-6 sm:px-7 sm:pb-7">
            {completed ? (
              <Button className="w-full" onClick={onCompleted}>
                <ArrowLeft aria-hidden="true" />{t('passwordReset.actions.backToLogin')}
              </Button>
            ) : tokenMissing ? (
              <div className="grid gap-4">
                <div role="alert" className="flex gap-2.5 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2.5 text-sm leading-5 text-destructive">
                  <AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                  <span>{t('passwordReset.errors.missingToken')}</span>
                </div>
                <Button variant="secondary" className="w-full" asChild>
                  <a href="#/forgot-password">{t('passwordReset.actions.requestAgain')}</a>
                </Button>
              </div>
            ) : (
              <form className="grid gap-5" onSubmit={handleSubmit} noValidate>
                <div className="grid gap-2">
                  <label htmlFor="reset-password-new" className="text-sm font-medium">{t('passwordReset.reset.password')}</label>
                  <div className="relative">
                    <Input
                      id="reset-password-new"
                      type={showPassword ? 'text' : 'password'}
                      autoComplete="new-password"
                      required
                      className="h-11 px-3 pr-11"
                      value={password}
                      aria-invalid={error === 'invalidInput' ? true : undefined}
                      onChange={(event) => {
                        setPassword(event.target.value)
                        setError(undefined)
                      }}
                    />
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon-sm"
                      className="absolute top-1.5 right-1.5 text-muted-foreground"
                      aria-label={passwordToggleLabel}
                      title={passwordToggleLabel}
                      aria-pressed={showPassword}
                      onClick={() => setShowPassword((current) => !current)}
                    >
                      {showPassword
                        ? <EyeOff className="size-4" aria-hidden="true" />
                        : <Eye className="size-4" aria-hidden="true" />}
                    </Button>
                  </div>
                  <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('passwordReset.reset.passwordHelp')}</p>
                </div>

                <div className="grid gap-2">
                  <label htmlFor="reset-password-confirm" className="text-sm font-medium">{t('passwordReset.reset.passwordConfirmation')}</label>
                  <Input
                    id="reset-password-confirm"
                    type={showPassword ? 'text' : 'password'}
                    autoComplete="new-password"
                    required
                    className="h-11 px-3"
                    value={passwordConfirmation}
                    aria-invalid={error === 'invalidInput' ? true : undefined}
                    onChange={(event) => {
                      setPasswordConfirmation(event.target.value)
                      setError(undefined)
                    }}
                  />
                </div>

                {error ? (
                  <div role="alert" aria-live="polite" className="flex gap-2.5 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2.5 text-sm leading-5 text-destructive">
                    <AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                    <span>{t(`passwordReset.errors.${error}`)}</span>
                  </div>
                ) : null}

                <Button type="submit" size="lg" className="w-full" disabled={mutation.isPending}>
                  {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
                  {t(mutation.isPending ? 'passwordReset.reset.submitting' : 'passwordReset.reset.submit')}
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

function classifyResetPasswordError(error: unknown): ResetPasswordError {
  if (!(error instanceof ApiError)) return 'unavailable'
  const code = errorCode(error.details)
  if (code === 'password_reset_rejected' || error.status === 409) return 'rejected'
  if (error.status === 400) return 'invalidInput'
  return 'unavailable'
}

function isValidPassword(value: string) {
  const bytes = new TextEncoder().encode(value).length
  return bytes >= 12 && bytes <= 128 && ![...value].some((character) => /\p{Cc}/u.test(character))
}

function errorCode(details: unknown) {
  if (typeof details !== 'object' || details === null || !('code' in details)) return undefined
  return typeof details.code === 'string' ? details.code : undefined
}
