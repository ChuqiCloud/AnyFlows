import { zodResolver } from '@hookform/resolvers/zod'
import { AlertCircle, Eye, EyeOff, LoaderCircle, UserRoundPlus } from 'lucide-react'
import { useState } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { establishManagementSession } from '@/features/auth/session-query'
import { ApiError } from '@/lib/api'
import { useRegisterUser } from './registration-api'
import {
  buildRegistrationFormSchema,
  defaultRegistrationFormValues,
  toRegistrationRequest,
  type RegistrationFormValues,
} from './registration-form-model'

type RegistrationFormProps = {
  emailRequired: boolean
  onAuthenticated: () => void
}

type RegistrationSubmitError =
  | 'conflict'
  | 'disabled'
  | 'invalidInput'
  | 'rateLimited'
  | 'unavailable'

/** 提交公开注册并只接受服务端签发的统一登录会话。 */
export function RegistrationForm({ emailRequired, onAuthenticated }: RegistrationFormProps) {
  const { t } = useTranslation()
  const [showPassword, setShowPassword] = useState(false)
  const [submitError, setSubmitError] = useState<RegistrationSubmitError>()
  const mutation = useRegisterUser()
  const form = useForm<RegistrationFormValues>({
    defaultValues: defaultRegistrationFormValues,
    resolver: zodResolver(buildRegistrationFormSchema(emailRequired, {
      invalidUsername: t('registration.validation.username'),
      invalidEmail: t('registration.validation.email'),
      emailRequired: t('registration.validation.emailRequired'),
      invalidPassword: t('registration.validation.password'),
      passwordMismatch: t('registration.validation.passwordMismatch'),
    })),
  })

  const onSubmit = form.handleSubmit(async (values) => {
    setSubmitError(undefined)
    try {
      const response = await mutation.mutateAsync(toRegistrationRequest(values))
      if (response.user.role !== 'user') {
        setSubmitError('unavailable')
        return
      }
      establishManagementSession(response)
      onAuthenticated()
    } catch (error) {
      setSubmitError(classifyRegistrationError(error))
    }
  })

  const passwordToggleLabel = showPassword
    ? t('registration.form.hidePassword')
    : t('registration.form.showPassword')

  return (
    <Card elevation="overlay" className="mx-auto w-full max-w-[430px] bg-card/94 backdrop-blur-2xl">
      <CardHeader className="gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
        <div className="mb-2 grid size-10 place-items-center rounded-xl border border-info/20 bg-info/10 text-info">
          <UserRoundPlus className="size-5" aria-hidden="true" />
        </div>
        <CardTitle className="text-xl leading-tight">{t('registration.form.title')}</CardTitle>
        <CardDescription className="leading-6">{t('registration.form.subtitle')}</CardDescription>
      </CardHeader>

      <CardContent className="px-6 pb-6 sm:px-7 sm:pb-7">
        <form className="grid gap-4" onSubmit={onSubmit} noValidate>
          <div className="grid gap-1.5">
            <label htmlFor="registration-username" className="text-sm font-medium">
              {t('registration.form.username')}
            </label>
            <Input
              id="registration-username"
              autoComplete="username"
              autoFocus
              required
              className="h-11 px-3"
              aria-invalid={!!form.formState.errors.username}
              aria-describedby={form.formState.errors.username ? 'registration-username-error' : undefined}
              {...form.register('username', { onChange: () => setSubmitError(undefined) })}
            />
            {form.formState.errors.username ? (
              <p id="registration-username-error" className="text-xs text-destructive">
                {form.formState.errors.username.message}
              </p>
            ) : null}
          </div>

          <div className="grid gap-1.5">
            <div className="flex items-center justify-between gap-3">
              <label htmlFor="registration-email" className="text-sm font-medium">
                {t('registration.form.email')}
              </label>
              {!emailRequired ? (
                <span className="text-[0.6875rem] text-muted-foreground">
                  {t('registration.form.optional')}
                </span>
              ) : null}
            </div>
            <Input
              id="registration-email"
              type="email"
              autoComplete="email"
              required={emailRequired}
              className="h-11 px-3"
              aria-invalid={!!form.formState.errors.email}
              aria-describedby={form.formState.errors.email ? 'registration-email-error' : 'registration-email-help'}
              {...form.register('email', { onChange: () => setSubmitError(undefined) })}
            />
            {form.formState.errors.email ? (
              <p id="registration-email-error" className="text-xs text-destructive">
                {form.formState.errors.email.message}
              </p>
            ) : (
              <p id="registration-email-help" className="text-[0.6875rem] leading-4 text-muted-foreground">
                {t('registration.form.emailHelp')}
              </p>
            )}
          </div>

          <div className="grid gap-1.5">
            <label htmlFor="registration-password" className="text-sm font-medium">
              {t('registration.form.password')}
            </label>
            <div className="relative">
              <Input
                id="registration-password"
                type={showPassword ? 'text' : 'password'}
                autoComplete="new-password"
                required
                className="h-11 px-3 pr-11"
                aria-invalid={!!form.formState.errors.password}
                aria-describedby={form.formState.errors.password ? 'registration-password-error' : 'registration-password-help'}
                {...form.register('password', { onChange: () => setSubmitError(undefined) })}
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
            {form.formState.errors.password ? (
              <p id="registration-password-error" className="text-xs text-destructive">
                {form.formState.errors.password.message}
              </p>
            ) : (
              <p id="registration-password-help" className="text-[0.6875rem] leading-4 text-muted-foreground">
                {t('registration.form.passwordHelp')}
              </p>
            )}
          </div>

          <div className="grid gap-1.5">
            <label htmlFor="registration-password-confirmation" className="text-sm font-medium">
              {t('registration.form.passwordConfirmation')}
            </label>
            <Input
              id="registration-password-confirmation"
              type={showPassword ? 'text' : 'password'}
              autoComplete="new-password"
              required
              className="h-11 px-3"
              aria-invalid={!!form.formState.errors.passwordConfirmation}
              aria-describedby={form.formState.errors.passwordConfirmation ? 'registration-password-confirmation-error' : undefined}
              {...form.register('passwordConfirmation', { onChange: () => setSubmitError(undefined) })}
            />
            {form.formState.errors.passwordConfirmation ? (
              <p id="registration-password-confirmation-error" className="text-xs text-destructive">
                {form.formState.errors.passwordConfirmation.message}
              </p>
            ) : null}
          </div>

          {submitError ? (
            <div
              role="alert"
              aria-live="polite"
              className="flex gap-2.5 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2.5 text-sm leading-5 text-destructive"
            >
              <AlertCircle className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
              <span>{t(`registration.errors.${submitError}`)}</span>
            </div>
          ) : null}

          <Button type="submit" size="lg" className="mt-1 w-full" disabled={mutation.isPending}>
            {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'registration.form.submitting' : 'registration.form.submit')}
          </Button>

          <p className="text-center text-sm text-muted-foreground">
            {t('registration.form.hasAccount')}{' '}
            <a href="#/login" className="font-medium text-info underline-offset-4 hover:underline">
              {t('registration.actions.login')}
            </a>
          </p>
        </form>
      </CardContent>
    </Card>
  )
}

function classifyRegistrationError(error: unknown): RegistrationSubmitError {
  if (!(error instanceof ApiError)) return 'unavailable'
  const code = errorCode(error.details)
  if (error.status === 409 || code === 'user_conflict') return 'conflict'
  if (code === 'registration_disabled') return 'disabled'
  if (error.status === 429 || code === 'registration_rate_limited') return 'rateLimited'
  if (error.status === 400) return 'invalidInput'
  return 'unavailable'
}

function errorCode(details: unknown) {
  if (typeof details !== 'object' || details === null || !('code' in details)) return undefined
  return typeof details.code === 'string' ? details.code : undefined
}
