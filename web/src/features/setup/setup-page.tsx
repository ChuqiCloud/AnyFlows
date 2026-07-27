import { useMutation } from '@tanstack/react-query'
import { AlertCircle, Eye, EyeOff, LoaderCircle, ShieldCheck } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { establishManagementSession } from '@/features/auth/session-query'
import { markInitialSetupComplete } from '@/features/setup/setup-query'
import { apiClient, ApiError } from '@/lib/api'
import { initializeAdminSetup } from '@/lib/api/generated/sdk.gen'
import type { SetupRequestWritable } from '@/lib/api/generated/types.gen'

type SetupPageProps = {
  onAuthenticated: () => void
  onConflict: () => void
}

type SetupError = 'invalidInput' | 'passwordMismatch' | 'unavailable'

const textEncoder = new TextEncoder()

function classifySetupError(error: unknown): SetupError | 'conflict' {
  if (error instanceof ApiError && error.status === 409) {
    return 'conflict'
  }
  if (error instanceof ApiError && error.status === 400) {
    return 'invalidInput'
  }
  return 'unavailable'
}

/** 收集首个管理员凭据，成功后直接建立管理会话。 */
export function SetupPage({ onAuthenticated, onConflict }: SetupPageProps) {
  const { t } = useTranslation()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [passwordConfirmation, setPasswordConfirmation] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [setupError, setSetupError] = useState<SetupError>()

  const setupMutation = useMutation({
    mutationFn: async (body: SetupRequestWritable) => {
      const { data } = await initializeAdminSetup({ body, client: apiClient })
      return data
    },
  })

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setSetupError(undefined)

    const usernameBytes = textEncoder.encode(username).length
    const passwordBytes = textEncoder.encode(password).length
    if (
      username.trim() !== username
      || usernameBytes === 0
      || usernameBytes > 64
      || passwordBytes < 12
      || passwordBytes > 128
    ) {
      setSetupError('invalidInput')
      return
    }
    if (password !== passwordConfirmation) {
      setSetupError('passwordMismatch')
      return
    }

    try {
      const response = await setupMutation.mutateAsync({ username, password })
      if (response.user.role !== 'admin') {
        setSetupError('unavailable')
        return
      }
      establishManagementSession(response)
      markInitialSetupComplete()
      onAuthenticated()
    } catch (error) {
      const classified = classifySetupError(error)
      if (classified === 'conflict') {
        markInitialSetupComplete()
        onConflict()
        return
      }
      setSetupError(classified)
    }
  }

  const passwordToggleLabel = showPassword
    ? t('setup.form.hidePassword')
    : t('setup.form.showPassword')
  const describedBy = setupError ? 'setup-password-help setup-error' : 'setup-password-help'

  return (
    <AuthPageShell>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_420px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('setup.hero.eyebrow')}</p>
          <h1 className="max-w-[11em] text-[2.75rem] leading-[1.08] font-semibold">
            {t('setup.hero.title')}
          </h1>
          <p className="mt-5 max-w-[36ch] text-base leading-7 text-muted-foreground">
            {t('setup.hero.body')}
          </p>
        </section>

        <Card elevation="overlay" className="mx-auto w-full max-w-[420px] bg-card/94 backdrop-blur-2xl">
          <CardHeader className="gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
            <div className="mb-2 grid size-10 place-items-center rounded-xl border border-info/20 bg-info/10 text-info">
              <ShieldCheck className="size-5" aria-hidden="true" />
            </div>
            <CardTitle className="text-xl leading-tight">{t('setup.form.title')}</CardTitle>
            <CardDescription className="leading-6">{t('setup.form.subtitle')}</CardDescription>
          </CardHeader>

          <CardContent className="px-6 pb-6 sm:px-7 sm:pb-7">
            <form className="grid gap-5" onSubmit={handleSubmit}>
              <div className="grid gap-2">
                <label htmlFor="setup-username" className="text-sm font-medium">
                  {t('setup.form.username')}
                </label>
                <Input
                  id="setup-username"
                  name="username"
                  autoComplete="username"
                  autoFocus
                  required
                  className="h-11 px-3"
                  placeholder={t('setup.form.usernamePlaceholder')}
                  value={username}
                  aria-invalid={setupError === 'invalidInput' ? true : undefined}
                  onChange={(event) => {
                    setUsername(event.target.value)
                    setSetupError(undefined)
                  }}
                />
              </div>

              <div className="grid gap-2">
                <label htmlFor="setup-password" className="text-sm font-medium">
                  {t('setup.form.password')}
                </label>
                <div className="relative">
                  <Input
                    id="setup-password"
                    name="password"
                    type={showPassword ? 'text' : 'password'}
                    autoComplete="new-password"
                    required
                    className="h-11 px-3 pr-11"
                    placeholder={t('setup.form.passwordPlaceholder')}
                    value={password}
                    aria-invalid={setupError ? true : undefined}
                    aria-describedby={describedBy}
                    onChange={(event) => {
                      setPassword(event.target.value)
                      setSetupError(undefined)
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
                <p id="setup-password-help" className="text-xs leading-5 text-muted-foreground">
                  {t('setup.form.passwordHelp')}
                </p>
              </div>

              <div className="grid gap-2">
                <label htmlFor="setup-password-confirmation" className="text-sm font-medium">
                  {t('setup.form.passwordConfirmation')}
                </label>
                <Input
                  id="setup-password-confirmation"
                  name="password-confirmation"
                  type={showPassword ? 'text' : 'password'}
                  autoComplete="new-password"
                  required
                  className="h-11 px-3"
                  placeholder={t('setup.form.passwordConfirmationPlaceholder')}
                  value={passwordConfirmation}
                  aria-invalid={setupError === 'passwordMismatch' ? true : undefined}
                  aria-describedby={setupError ? 'setup-error' : undefined}
                  onChange={(event) => {
                    setPasswordConfirmation(event.target.value)
                    setSetupError(undefined)
                  }}
                />
              </div>

              {setupError ? (
                <div
                  id="setup-error"
                  role="alert"
                  aria-live="polite"
                  className="flex gap-2.5 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2.5 text-sm leading-5 text-destructive"
                >
                  <AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                  <span>{t(`setup.errors.${setupError}`)}</span>
                </div>
              ) : null}

              <Button type="submit" size="lg" className="w-full" disabled={setupMutation.isPending}>
                {setupMutation.isPending ? (
                  <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
                ) : null}
                {setupMutation.isPending ? t('setup.form.submitting') : t('setup.form.submit')}
              </Button>
            </form>
          </CardContent>
        </Card>
      </div>
    </AuthPageShell>
  )
}
