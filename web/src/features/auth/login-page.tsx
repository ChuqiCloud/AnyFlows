import { useMutation } from '@tanstack/react-query'
import { AlertCircle, Boxes, Eye, EyeOff, LoaderCircle, LockKeyhole, UserRoundPlus } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { establishManagementSession } from '@/features/auth/session-query'
import { useRegistrationStatus } from '@/features/registration/registration-api'
import { apiClient, ApiError } from '@/lib/api'
import { loginManagementSession } from '@/lib/api/generated/sdk.gen'
import type { LoginRequest } from '@/lib/api/generated/types.gen'

export type LoginNotice = 'sessionExpired' | 'setupComplete'

type LoginPageProps = {
  notice?: LoginNotice
  onAuthenticated: () => void
}

type LoginError = LoginNotice | 'invalidCredentials' | 'unavailable'

function classifyLoginError(error: unknown): LoginError {
  if (error instanceof ApiError && (error.status === 400 || error.status === 401)) {
    return 'invalidCredentials'
  }

  return 'unavailable'
}

export function LoginPage({ notice, onAuthenticated }: LoginPageProps) {
  const { t } = useTranslation()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [loginError, setLoginError] = useState<LoginError>()
  const [noticeDismissed, setNoticeDismissed] = useState(false)
  const registrationStatus = useRegistrationStatus()

  const loginMutation = useMutation({
    mutationFn: async (body: LoginRequest) => {
      const { data } = await loginManagementSession({ body, client: apiClient })
      return data
    },
  })

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setLoginError(undefined)

    try {
      const response = await loginMutation.mutateAsync({ username, password })

      establishManagementSession(response)
      onAuthenticated()
    } catch (error) {
      setLoginError(classifyLoginError(error))
    }
  }

  const visibleError = loginError ?? (noticeDismissed ? undefined : notice)
  const hasFieldError = visibleError === 'invalidCredentials'
  const passwordToggleLabel = showPassword
    ? t('auth.login.hidePassword')
    : t('auth.login.showPassword')

  return (
    <AuthPageShell>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_400px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('auth.login.eyebrow')}</p>
          <h1 className="max-w-[10em] text-[2.75rem] leading-[1.08] font-semibold">
            {t('auth.login.heroTitle')}
          </h1>
          <p className="mt-5 max-w-[34ch] text-base leading-7 text-muted-foreground">
            {t('auth.login.heroBody')}
          </p>
        </section>

        <Card
          elevation="overlay"
          className="mx-auto w-full max-w-[400px] bg-card/94 backdrop-blur-2xl"
        >
          <CardHeader className="gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
            <div className="mb-2 grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-2 text-info">
              <LockKeyhole className="size-5" aria-hidden="true" />
            </div>
            <CardTitle className="text-xl leading-tight">{t('auth.login.title')}</CardTitle>
            <CardDescription className="leading-6">{t('auth.login.subtitle')}</CardDescription>
          </CardHeader>

          <CardContent className="px-6 pb-6 sm:px-7 sm:pb-7">
            <form className="grid gap-5" onSubmit={handleSubmit}>
              <div className="grid gap-2">
                <label htmlFor="management-username" className="text-sm font-medium">
                  {t('auth.login.username')}
                </label>
                <Input
                  id="management-username"
                  name="username"
                  autoComplete="username"
                  autoFocus
                  required
                  className="h-11 px-3"
                  placeholder={t('auth.login.usernamePlaceholder')}
                  value={username}
                  aria-invalid={hasFieldError ? true : undefined}
                  aria-describedby={visibleError ? 'login-error' : undefined}
                  onChange={(event) => {
                    setUsername(event.target.value)
                    setLoginError(undefined)
                    setNoticeDismissed(true)
                  }}
                />
              </div>

              <div className="grid gap-2">
                <label htmlFor="management-password" className="text-sm font-medium">
                  {t('auth.login.password')}
                </label>
                <div className="relative">
                  <Input
                    id="management-password"
                    name="password"
                    type={showPassword ? 'text' : 'password'}
                    autoComplete="current-password"
                    required
                    className="h-11 px-3 pr-11"
                    placeholder={t('auth.login.passwordPlaceholder')}
                    value={password}
                    aria-invalid={hasFieldError ? true : undefined}
                    aria-describedby={visibleError ? 'login-error' : undefined}
                    onChange={(event) => {
                      setPassword(event.target.value)
                      setLoginError(undefined)
                      setNoticeDismissed(true)
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
                    {showPassword ? (
                      <EyeOff className="size-4" aria-hidden="true" />
                    ) : (
                      <Eye className="size-4" aria-hidden="true" />
                    )}
                  </Button>
                </div>
              </div>

              {visibleError ? (
                <div
                  id="login-error"
                  role="alert"
                  aria-live="polite"
                  className="flex gap-2.5 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2.5 text-sm leading-5 text-destructive"
                >
                  <AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                  <span>{t(`auth.errors.${visibleError}`)}</span>
                </div>
              ) : null}

              <Button type="submit" size="lg" className="w-full" disabled={loginMutation.isPending}>
                {loginMutation.isPending ? (
                  <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
                ) : null}
                {loginMutation.isPending ? t('auth.login.submitting') : t('auth.login.submit')}
              </Button>
              {registrationStatus.data?.enabled ? (
                <p className="text-center text-sm text-muted-foreground">
                  {t('auth.login.noAccount')}{' '}
                  <a href="#/register" className="font-medium text-info underline-offset-4 hover:underline">
                    <UserRoundPlus className="mr-1 inline size-3.5" aria-hidden="true" />
                    {t('auth.login.register')}
                  </a>
                </p>
              ) : null}
              <Button variant="ghost" className="w-full" asChild>
                <a href="#/models">
                  <Boxes className="size-4" aria-hidden="true" />
                  {t('auth.login.browseModels')}
                </a>
              </Button>
            </form>
          </CardContent>
        </Card>
      </div>
    </AuthPageShell>
  )
}
