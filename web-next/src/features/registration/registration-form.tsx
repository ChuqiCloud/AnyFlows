import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Card, CardBody, CardHeader, Chip, Input } from '@heroui/react'
import {
  AlertCircle,
  Eye,
  EyeOff,
  LoaderCircle,
  MailCheck,
  TicketCheck,
  UserRoundPlus,
} from 'lucide-react'
import { useEffect, useState } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { establishManagementSession } from '@/features/auth/session-query'
import { TurnstileWidget } from '@/features/auth/turnstile-widget'
import { ApiError } from '@/lib/api'
import {
  useRegisterUser,
  useSendRegistrationEmailVerification,
} from './registration-api'
import {
  buildRegistrationFormSchema,
  defaultRegistrationFormValues,
  toRegistrationRequest,
  type RegistrationFormValues,
} from './registration-form-model'

type RegistrationFormProps = {
  emailRequired: boolean
  turnstileSiteKey: string | null
  initialInviteCode?: string
  onAuthenticated: () => void
}

type RegistrationSubmitError =
  | 'conflict'
  | 'disabled'
  | 'invalidInput'
  | 'invitationRejected'
  | 'verificationRejected'
  | 'turnstileRequired'
  | 'turnstileRejected'
  | 'turnstileUnavailable'
  | 'rateLimited'
  | 'unavailable'

type RegistrationVerificationError =
  | 'disabled'
  | 'emailNotConfigured'
  | 'emailDeliveryFailed'
  | 'invalidInput'
  | 'turnstileRequired'
  | 'turnstileRejected'
  | 'turnstileUnavailable'
  | 'rateLimited'
  | 'unavailable'

const verificationCooldownStorageKey = 'anyflows.registration.verification.nextSendAt'

/** 提交公开注册并只接受服务端签发的统一登录会话。 */
export function RegistrationForm({
  emailRequired,
  turnstileSiteKey,
  initialInviteCode,
  onAuthenticated,
}: RegistrationFormProps) {
  const { t } = useTranslation()
  const [showPassword, setShowPassword] = useState(false)
  const [submitError, setSubmitError] = useState<RegistrationSubmitError>()
  const [verificationError, setVerificationError] = useState<RegistrationVerificationError>()
  const [verificationNotice, setVerificationNotice] = useState(false)
  const [nextSendAt, setNextSendAt] = useState(readVerificationCooldown)
  const [turnstileToken, setTurnstileToken] = useState('')
  const [turnstileResetKey, setTurnstileResetKey] = useState(0)
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))
  const mutation = useRegisterUser()
  const sendVerificationMutation = useSendRegistrationEmailVerification()
  const form = useForm<RegistrationFormValues>({
    defaultValues: {
      ...defaultRegistrationFormValues,
      inviteCode: initialInviteCode ?? '',
    },
    resolver: zodResolver(buildRegistrationFormSchema(emailRequired, {
      invalidUsername: t('registration.validation.username'),
      invalidEmail: t('registration.validation.email'),
      emailRequired: t('registration.validation.emailRequired'),
      invalidVerificationCode: t('registration.validation.verificationCode'),
      verificationCodeRequired: t('registration.validation.verificationCodeRequired'),
      invalidInviteCode: t('registration.validation.inviteCode'),
      invalidPassword: t('registration.validation.password'),
      passwordMismatch: t('registration.validation.passwordMismatch'),
    })),
  })
  const email = form.watch('email')
  const cooldownSeconds = Math.max(0, nextSendAt - now)
  const hasEmail = email.trim().length > 0

  useEffect(() => {
    if (cooldownSeconds === 0) return undefined
    const timer = window.setInterval(() => {
      setNow(Math.floor(Date.now() / 1000))
    }, 1000)
    return () => window.clearInterval(timer)
  }, [cooldownSeconds])

  useEffect(() => {
    if (nextSendAt > 0 && cooldownSeconds === 0) {
      writeVerificationCooldown(0)
    }
  }, [cooldownSeconds, nextSendAt])

  useEffect(() => {
    if (initialInviteCode) {
      form.setValue('inviteCode', initialInviteCode, { shouldValidate: true })
    }
  }, [form, initialInviteCode])

  const onSendVerification = async () => {
    setVerificationError(undefined)
    setVerificationNotice(false)
    if (!emailRequired) return
    if (cooldownSeconds > 0 || !(await form.trigger('email'))) return
    if (turnstileSiteKey && !turnstileToken) {
      setVerificationError('turnstileRequired')
      return
    }
    try {
      const response = await sendVerificationMutation.mutateAsync({
        email: form.getValues('email'),
        ...(turnstileToken ? { turnstile_token: turnstileToken } : {}),
      })
      setNextSendAt(response.next_send_at)
      setNow(Math.floor(Date.now() / 1000))
      writeVerificationCooldown(response.next_send_at)
      setVerificationNotice(true)
    } catch (error) {
      setVerificationError(classifyVerificationError(error))
      if (error instanceof ApiError && error.status === 429) {
        const retryAfter = retryAfterSeconds(error.details)
        if (retryAfter > 0) {
          const serverBoundary = Math.floor(Date.now() / 1000) + retryAfter
          setNextSendAt(serverBoundary)
          writeVerificationCooldown(serverBoundary)
        }
      }
    } finally {
      if (turnstileToken) {
        setTurnstileToken('')
        setTurnstileResetKey((value) => value + 1)
      }
    }
  }

  const onSubmit = form.handleSubmit(async (values) => {
    setSubmitError(undefined)
    if (turnstileSiteKey && !turnstileToken) {
      setSubmitError('turnstileRequired')
      return
    }
    try {
      const response = await mutation.mutateAsync(toRegistrationRequest(values, emailRequired, turnstileToken))
      if (response.user.role !== 'user') {
        setSubmitError('unavailable')
        return
      }
      establishManagementSession(response)
      onAuthenticated()
    } catch (error) {
      setSubmitError(classifyRegistrationError(error))
    } finally {
      if (turnstileToken) {
        setTurnstileToken('')
        setTurnstileResetKey((value) => value + 1)
      }
    }
  })

  const passwordToggleLabel = showPassword
    ? t('registration.form.hidePassword')
    : t('registration.form.showPassword')

  return (
    <Card className="mx-auto w-full max-w-[430px] border border-[var(--hairline)] bg-card/94 backdrop-blur-2xl" shadow="none">
      <CardHeader className="flex flex-col gap-2 px-6 pt-6 pb-5 sm:px-7 sm:pt-7">
        <div className="mb-2 grid size-10 place-items-center rounded-xl border border-info/20 bg-info/10 text-info">
          <UserRoundPlus className="size-5" aria-hidden="true" />
        </div>
        <h2 className="text-xl leading-tight font-semibold">{t('registration.form.title')}</h2>
        <p className="text-sm leading-6 text-muted-foreground">{t('registration.form.subtitle')}</p>
      </CardHeader>

      <CardBody className="px-6 pb-6 sm:px-7 sm:pb-7">
        <form className="grid gap-4" onSubmit={onSubmit} noValidate>
          <div className="grid gap-1.5">
            <label htmlFor="registration-username" className="text-sm font-medium">
              {t('registration.form.username')}
            </label>
            <Input
              className="h-11 px-3"
              id="registration-username"
              autoComplete="username"
              autoFocus
              isInvalid={!!form.formState.errors.username}
              required
              {...form.register('username', { onChange: () => setSubmitError(undefined) })}
            />
            {form.formState.errors.username ? (
              <p id="registration-username-error" className="text-xs text-destructive">
                {form.formState.errors.username.message}
              </p>
            ) : null}
          </div>

          {emailRequired ? <div className="grid gap-1.5">
            <label htmlFor="registration-email" className="text-sm font-medium">{t('registration.form.email')}</label>
            <Input
              className="h-11 px-3"
              id="registration-email"
              aria-describedby={form.formState.errors.email ? 'registration-email-error' : 'registration-email-help'}
              autoComplete="email"
              isInvalid={!!form.formState.errors.email}
              required={emailRequired}
              type="email"
              {...form.register('email', {
                onChange: () => {
                  setSubmitError(undefined)
                  setVerificationError(undefined)
                  setVerificationNotice(false)
                },
              })}
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
          </div> : null}

          {turnstileSiteKey ? (
            <TurnstileWidget
              siteKey={turnstileSiteKey}
              resetKey={turnstileResetKey}
              onToken={setTurnstileToken}
            />
          ) : null}

          {emailRequired && hasEmail ? (
            <div className="grid gap-1.5">
              <div className="flex items-center justify-between gap-3">
                <label htmlFor="registration-verification-code" className="text-sm font-medium">
                  {t('registration.form.verificationCode')}
                </label>
                <span className="text-[0.6875rem] text-muted-foreground">
                  {t('registration.form.verificationCodeHint')}
                </span>
              </div>
              <div className="flex gap-2">
                <Input
                  className="h-11 min-w-0 flex-1 px-3 tracking-[0.28em]"
                  id="registration-verification-code"
                  aria-describedby={form.formState.errors.verificationCode ? 'registration-verification-code-error' : 'registration-verification-code-help'}
                  autoComplete="one-time-code"
                  inputMode="numeric"
                  isInvalid={!!form.formState.errors.verificationCode}
                  maxLength={6}
                  {...form.register('verificationCode', {
                    onChange: () => {
                      setSubmitError(undefined)
                      setVerificationError(undefined)
                    },
                  })}
                />
                <Button
                  type="button"
                  variant="bordered"
                  className="h-11 shrink-0 px-3"
                  isDisabled={sendVerificationMutation.isPending || cooldownSeconds > 0}
                  onClick={onSendVerification}
                >
                  {sendVerificationMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <MailCheck className="size-4" aria-hidden="true" />}
                  <span className="sr-only sm:not-sr-only">
                    {sendVerificationMutation.isPending
                      ? t('registration.form.sendingVerification')
                      : cooldownSeconds > 0
                        ? t('registration.form.resendIn', { seconds: cooldownSeconds })
                        : t('registration.form.sendVerification')}
                  </span>
                </Button>
              </div>
              {form.formState.errors.verificationCode ? (
                <p id="registration-verification-code-error" className="text-xs text-destructive">
                  {form.formState.errors.verificationCode.message}
                </p>
              ) : verificationError ? (
                <p id="registration-verification-code-help" className="text-xs text-destructive">
                  {t(`registration.verificationErrors.${verificationError}`)}
                </p>
              ) : verificationNotice ? (
                <p id="registration-verification-code-help" className="text-xs text-success">
                  {t('registration.form.verificationSent')}
                </p>
              ) : cooldownSeconds > 0 ? (
                <p id="registration-verification-code-help" className="text-xs text-muted-foreground">
                  {t('registration.form.resendIn', { seconds: cooldownSeconds })}
                </p>
              ) : (
                <p id="registration-verification-code-help" className="text-[0.6875rem] leading-4 text-muted-foreground">
                  {t('registration.form.verificationCodeHelp')}
                </p>
              )}
            </div>
          ) : null}

          <div className="grid gap-1.5">
            <div className="flex items-center justify-between gap-3">
              <label htmlFor="registration-invite-code" className="text-sm font-medium">
                {t('registration.form.inviteCode')}
              </label>
              {initialInviteCode ? (
                <Chip className="border-success/25 bg-success/10 text-success" size="sm" variant="flat">
                  {t('registration.form.inviteApplied')}
                </Chip>
              ) : (
                <span className="text-[0.6875rem] text-muted-foreground">
                  {t('registration.form.optional')}
                </span>
              )}
            </div>
            <div className="relative">
              <TicketCheck
                className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
                aria-hidden="true"
              />
              <Input
                className="h-11 px-3 pl-10 font-mono"
                id="registration-invite-code"
                aria-describedby={form.formState.errors.inviteCode ? 'registration-invite-code-error' : 'registration-invite-code-help'}
                autoComplete="off"
                isInvalid={!!form.formState.errors.inviteCode}
                maxLength={25}
                {...form.register('inviteCode', { onChange: () => setSubmitError(undefined) })}
              />
            </div>
            {form.formState.errors.inviteCode ? (
              <p id="registration-invite-code-error" className="text-xs text-destructive">
                {form.formState.errors.inviteCode.message}
              </p>
            ) : (
              <p id="registration-invite-code-help" className="text-[0.6875rem] leading-4 text-muted-foreground">
                {t('registration.form.inviteCodeHelp')}
              </p>
            )}
          </div>

          <div className="grid gap-1.5">
            <label htmlFor="registration-password" className="text-sm font-medium">
              {t('registration.form.password')}
            </label>
            <div className="relative">
              <Input
                className="h-11 px-3 pr-11"
                id="registration-password"
                aria-describedby={form.formState.errors.password ? 'registration-password-error' : 'registration-password-help'}
                autoComplete="new-password"
                isInvalid={!!form.formState.errors.password}
                required
                type={showPassword ? 'text' : 'password'}
                {...form.register('password', { onChange: () => setSubmitError(undefined) })}
              />
              <Button
                isIconOnly
                aria-label={passwordToggleLabel}
                aria-pressed={showPassword}
                className="absolute top-1.5 right-1.5"
                size="md"
                title={passwordToggleLabel}
                type="button"
                variant="light"
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
              className="h-11 px-3"
              id="registration-password-confirmation"
              aria-describedby={form.formState.errors.passwordConfirmation ? 'registration-password-confirmation-error' : undefined}
              autoComplete="new-password"
              isInvalid={!!form.formState.errors.passwordConfirmation}
              required
              type={showPassword ? 'text' : 'password'}
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

          <Button type="submit" color="primary" size="lg" className="mt-1 w-full" isDisabled={mutation.isPending}>
            {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'registration.form.submitting' : 'registration.form.submit')}
          </Button>

          <p className="text-center text-sm text-muted-foreground">
            {t('registration.form.hasAccount')}{' '}
            <a href="/login" className="font-medium text-info underline-offset-4 hover:underline">
              {t('registration.actions.login')}
            </a>
          </p>
        </form>
      </CardBody>
    </Card>
  )
}

function classifyRegistrationError(error: unknown): RegistrationSubmitError {
  if (!(error instanceof ApiError)) return 'unavailable'
  const code = errorCode(error.details)
  if (code === 'registration_rejected') return 'verificationRejected'
  if (code === 'invitation_rejected') return 'invitationRejected'
  if (code === 'turnstile_rejected') return 'turnstileRejected'
  if (code === 'turnstile_unavailable') return 'turnstileUnavailable'
  if (error.status === 409 || code === 'user_conflict') return 'conflict'
  if (code === 'registration_disabled') return 'disabled'
  if (error.status === 429 || code === 'registration_rate_limited') return 'rateLimited'
  if (error.status === 400) return 'invalidInput'
  return 'unavailable'
}

function classifyVerificationError(error: unknown): RegistrationVerificationError {
  if (!(error instanceof ApiError)) return 'unavailable'
  const code = errorCode(error.details)
  if (code === 'registration_disabled') return 'disabled'
  if (code === 'email_not_configured') return 'emailNotConfigured'
  if (code === 'email_delivery_failed') return 'emailDeliveryFailed'
  if (code === 'turnstile_rejected') return 'turnstileRejected'
  if (code === 'turnstile_unavailable') return 'turnstileUnavailable'
  if (error.status === 429 || code === 'registration_rate_limited') return 'rateLimited'
  if (error.status === 400) return 'invalidInput'
  return 'unavailable'
}

function retryAfterSeconds(details: unknown) {
  if (typeof details !== 'object' || details === null || !('retry_after_seconds' in details)) return 0
  const value = details.retry_after_seconds
  return typeof value === 'number' && Number.isFinite(value) ? Math.max(0, Math.ceil(value)) : 0
}

function readVerificationCooldown() {
  try {
    const value = Number(globalThis.sessionStorage.getItem(verificationCooldownStorageKey))
    return Number.isFinite(value) && value > 0 ? Math.floor(value) : 0
  } catch {
    return 0
  }
}

function writeVerificationCooldown(value: number) {
  try {
    if (value > 0) globalThis.sessionStorage.setItem(verificationCooldownStorageKey, String(value))
    else globalThis.sessionStorage.removeItem(verificationCooldownStorageKey)
  } catch {
    // 浏览器隐私模式禁止 sessionStorage 时仍以服务端响应为准。
  }
}

function errorCode(details: unknown) {
  if (typeof details !== 'object' || details === null || !('code' in details)) return undefined
  return typeof details.code === 'string' ? details.code : undefined
}
