import { useMutation } from '@tanstack/react-query'
import {
  AlertCircle,
  Boxes,
  CheckCircle2,
  Eye,
  EyeOff,
  GitFork,
  LoaderCircle,
  LockKeyhole,
  RefreshCw,
  UserRoundPlus,
} from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { AuthCapabilityCard, AuthCapabilitySkeleton } from '@/features/auth/auth-capability-card'
import { AuthPageShell } from '@/features/auth/auth-page-shell'
import { beginGitHubOAuthLogin } from '@/features/auth/oauth-login-api'
import { establishManagementSession } from '@/features/auth/session-query'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { apiClient, ApiError } from '@/lib/api'
import { loginManagementSession } from '@/lib/api/generated/sdk.gen'
import type { LoginRequest } from '@/lib/api/generated/types.gen'

export type LoginNotice = 'sessionExpired' | 'setupComplete' | 'passwordResetSuccess' | 'passwordChanged'

type LoginPageProps = {
  notice?: LoginNotice
  onAuthenticated: () => void
}

type LoginError = LoginNotice | 'invalidCredentials' | 'passwordLoginDisabled' | 'unavailable'

function classifyLoginError(error: unknown): LoginError {
  if (error instanceof ApiError) {
    const code = errorCode(error.details)
    // 兼容网关将管理错误包裹到 error 字段后的响应，避免把可解释的认证失败误报为服务不可用。
    if (code === 'password_login_disabled') return 'passwordLoginDisabled'
    if (code === 'invalid_credentials') return 'invalidCredentials'
    if (error.status === 400 || error.status === 401) return 'invalidCredentials'
  }
  return 'unavailable'
}

/** 根据公开认证能力决定是否渲染密码表单，未知状态下保持失败关闭。 */
export function LoginPage({ notice, onAuthenticated }: LoginPageProps) {
  const { t } = useTranslation()
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const [loginError, setLoginError] = useState<LoginError>()
  const [noticeDismissed, setNoticeDismissed] = useState(false)
  const siteQuery = usePublicSiteSettings()

  const loginMutation = useMutation({
    mutationFn: async (body: LoginRequest) => {
      const { data } = await loginManagementSession({ body, client: apiClient })
      return data
    },
  })
  const oauthMutation = useMutation({ mutationFn: beginGitHubOAuthLogin })

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

  async function handleGitHubLogin() {
    setLoginError(undefined)
    try {
      const response = await oauthMutation.mutateAsync()
      window.location.assign(response.authorization_url)
    } catch {
      setLoginError('unavailable')
    }
  }

  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')
  const heroTitle = site?.brand.tagline ?? t('auth.login.heroTitle')
  const heroBody = site?.brand.description ?? t('auth.login.heroBody')
  const passwordLoginEnabled = site?.authentication.password_login_enabled ?? false
  const githubLoginEnabled = site?.authentication.oauth_providers?.some(
    (provider) => provider.id === 'github',
  ) ?? false
  const registrationEnabled = site?.authentication.registration_enabled ?? false
  const registrationAvailable = passwordLoginEnabled && registrationEnabled
  const visibleError = loginError ?? (noticeDismissed ? undefined : notice)
  const hasFieldError = visibleError === 'invalidCredentials'
  const isSuccessNotice = visibleError === 'passwordResetSuccess' || visibleError === 'passwordChanged'
  const passwordToggleLabel = showPassword
    ? t('auth.login.hidePassword')
    : t('auth.login.showPassword')

  return (
    <AuthPageShell site={site}>
      <div className="grid w-full items-center gap-12 lg:grid-cols-[minmax(0,1fr)_400px] lg:gap-20">
        <section className="hidden max-w-[34rem] lg:block">
          <p className="mb-4 text-sm font-medium text-info">{t('auth.login.eyebrow', { siteName })}</p>
          <h1 className="max-w-[11em] text-[2.75rem] leading-[1.08] font-semibold">{heroTitle}</h1>
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
                <Button type="button" onClick={() => void siteQuery.refetch()}>
                  <RefreshCw aria-hidden="true" />
                  {t('auth.login.retry')}
                </Button>
                <Button variant="secondary" asChild>
                  <a href="#/models"><Boxes aria-hidden="true" />{t('auth.login.browseModels')}</a>
                </Button>
              </div>
            )}
          />
        ) : !passwordLoginEnabled && !githubLoginEnabled ? (
          <AuthCapabilityCard
            icon={<LockKeyhole className="size-5" aria-hidden="true" />}
            title={t('auth.login.disabledTitle')}
            body={t('auth.login.disabledBody')}
            action={(
              <Button variant="secondary" className="w-full" asChild>
                <a href="#/models"><Boxes aria-hidden="true" />{t('auth.login.browseModels')}</a>
              </Button>
            )}
          />
        ) : (
          <Card elevation="overlay" className="mx-auto w-full max-w-[400px] bg-card/94 backdrop-blur-2xl">
            <CardHeader className="gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
              <div className="mb-2 grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-2 text-info">
                <LockKeyhole className="size-5" aria-hidden="true" />
              </div>
              <CardTitle className="text-xl leading-tight">{t('auth.login.title')}</CardTitle>
              <CardDescription className="leading-6">{t('auth.login.subtitle')}</CardDescription>
            </CardHeader>

            <CardContent className="px-6 pb-6 sm:px-7 sm:pb-7">
              <form className="grid gap-5" onSubmit={handleSubmit}>
                {githubLoginEnabled ? (
                  <Button
                    type="button"
                    size="lg"
                    variant="secondary"
                    className="w-full"
                    disabled={oauthMutation.isPending}
                    onClick={() => void handleGitHubLogin()}
                  >
                    {oauthMutation.isPending
                      ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" />
                      : <GitFork className="size-4" aria-hidden="true" />}
                    {oauthMutation.isPending ? t('auth.login.oauthStarting') : t('auth.login.github')}
                  </Button>
                ) : null}

                {githubLoginEnabled && passwordLoginEnabled ? (
                  <div className="flex items-center gap-3 text-xs text-muted-foreground" aria-hidden="true">
                    <span className="h-px flex-1 bg-[var(--hairline)]" />
                    <span>{t('auth.login.orPassword')}</span>
                    <span className="h-px flex-1 bg-[var(--hairline)]" />
                  </div>
                ) : null}

                {visibleError ? (
                  <div
                    id="login-error"
                    role="alert"
                    aria-live="polite"
                    className={isSuccessNotice
                      ? 'flex gap-2.5 rounded-lg border border-success/20 bg-success/8 px-3 py-2.5 text-sm leading-5 text-success'
                      : 'flex gap-2.5 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2.5 text-sm leading-5 text-destructive'}
                  >
                    {isSuccessNotice
                      ? <CheckCircle2 className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                      : <AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" />}
                    <span>{t(`auth.errors.${visibleError}`)}</span>
                  </div>
                ) : null}

                {passwordLoginEnabled ? (
                  <>
                <div className="grid gap-2">
                  <label htmlFor="management-username" className="text-sm font-medium">{t('auth.login.username')}</label>
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
                  <label htmlFor="management-password" className="text-sm font-medium">{t('auth.login.password')}</label>
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
                      {showPassword
                        ? <EyeOff className="size-4" aria-hidden="true" />
                        : <Eye className="size-4" aria-hidden="true" />}
                    </Button>
                  </div>
                </div>

                <Button type="submit" size="lg" className="w-full" disabled={loginMutation.isPending}>
                  {loginMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
                  {loginMutation.isPending ? t('auth.login.submitting') : t('auth.login.submit')}
                </Button>
                <p className="text-center text-sm">
                  <a href="#/forgot-password" className="font-medium text-info underline-offset-4 hover:underline">
                    {t('auth.login.forgotPassword')}
                  </a>
                </p>
                  </>
                ) : null}
                {registrationAvailable ? (
                  <p className="text-center text-sm text-muted-foreground">
                    {t('auth.login.noAccount')}{' '}
                    <a href="#/register" className="font-medium text-info underline-offset-4 hover:underline">
                      <UserRoundPlus className="mr-1 inline size-3.5" aria-hidden="true" />
                      {t('auth.login.register')}
                    </a>
                  </p>
                ) : null}
                <Button variant="ghost" className="w-full" asChild>
                  <a href="#/models"><Boxes className="size-4" aria-hidden="true" />{t('auth.login.browseModels')}</a>
                </Button>
              </form>
            </CardContent>
          </Card>
        )}
      </div>
    </AuthPageShell>
  )
}

function errorCode(details: unknown) {
  if (typeof details !== 'object' || details === null) return undefined
  if ('code' in details && typeof details.code === 'string') return details.code
  if ('error' in details && typeof details.error === 'object' && details.error !== null && 'code' in details.error) {
    return typeof details.error.code === 'string' ? details.error.code : undefined
  }
  return undefined
}
