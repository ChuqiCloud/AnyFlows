import { Button, Card, CardBody, CardHeader, Chip, Divider, Input, Select, SelectItem, Skeleton, Switch } from '@heroui/react'
import { type FormEvent, type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import {
  BellRing,
  CalendarClock,
  Check,
  CircleDollarSign,
  Copy,
  Fingerprint,
  KeyRound,
  LoaderCircle,
  Mail,
  Pencil,
  RefreshCw,
  ShieldCheck,
  ShieldOff,
  Trash2,
  UserRound,
  X,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ApiError } from '@/lib/api'
import { copyText } from '@/lib/clipboard'
import type {
  UserPasskeyRegistrationOptionsResponse,
  UserPasskeyResponse,
  UserNotification,
  UserProfileResponse,
  UserTwoFactorEnrollmentResponse,
} from '@/lib/api/generated/types.gen'
import { isValidProfilePassword, isValidProfileUsername } from './profile-form-model'
import {
  useChangeUserPassword,
  useConfirmUserEmailBinding,
  useDisableUserTwoFactor,
  useEnableUserTwoFactor,
  useFinishUserPasskeyRegistration,
  useMarkUserNotificationsRead,
  useRenameUserPasskey,
  useRevokeUserPasskey,
  useStartUserPasskeyRegistration,
  useSendUserEmailBindingVerification,
  useUpdateUserNotificationPreferences,
  useUpdateUserProfile,
  useUserPasskeys,
  useUserNotifications,
  useUserProfile,
  useUserTwoFactor,
} from './profile-api'

type ProfilePageProps = {
  section?: 'notifications'
  onPasswordChanged: () => void
}

function errorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  const details = error.details as Record<string, unknown>
  return typeof details.code === 'string' ? details.code : undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function base64UrlToBuffer(value: string): ArrayBuffer {
  const normalized = value.replace(/-/g, '+').replace(/_/g, '/')
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, '=')
  const decoded = atob(padded)
  const bytes = new Uint8Array(decoded.length)
  for (let index = 0; index < decoded.length; index += 1) bytes[index] = decoded.charCodeAt(index)
  return bytes.buffer
}

function bufferToBase64Url(value: ArrayBuffer): string {
  const bytes = new Uint8Array(value)
  let binary = ''
  for (const byte of bytes) binary += String.fromCharCode(byte)
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/u, '')
}

/** 把服务端 JSON 中的 Base64URL 字段恢复成 WebAuthn API 所需的二进制值。 */
function toCreationOptions(response: UserPasskeyRegistrationOptionsResponse): PublicKeyCredentialCreationOptions {
  const value = response.options
  const root = isRecord(value) && isRecord(value.publicKey)
    ? value.publicKey
    : isRecord(value)
      ? value
      : {}
  const options: Record<string, unknown> = { ...root }
  if (typeof options.challenge === 'string') options.challenge = base64UrlToBuffer(options.challenge)
  if (isRecord(options.user) && typeof options.user.id === 'string') {
    options.user = { ...options.user, id: base64UrlToBuffer(options.user.id) }
  }
  if (Array.isArray(options.excludeCredentials)) {
    options.excludeCredentials = options.excludeCredentials.map((credential: unknown) => (
      isRecord(credential) && typeof credential.id === 'string'
        ? { ...credential, id: base64UrlToBuffer(credential.id) }
        : credential
    ))
  }
  return options as unknown as PublicKeyCredentialCreationOptions
}

function serializeRegistrationCredential(credential: PublicKeyCredential): Record<string, unknown> {
  const response = credential.response
  if (!(response instanceof AuthenticatorAttestationResponse)) throw new Error('invalid-credential')
  return {
    id: credential.id,
    rawId: bufferToBase64Url(credential.rawId),
    type: credential.type,
    response: {
      clientDataJSON: bufferToBase64Url(response.clientDataJSON),
      attestationObject: bufferToBase64Url(response.attestationObject),
      ...(typeof response.getTransports === 'function' ? { transports: response.getTransports() } : {}),
    },
  }
}

function supportsPasskeys() {
  return typeof window !== 'undefined'
    && 'PublicKeyCredential' in window
    && typeof navigator.credentials?.create === 'function'
}

function formatPasskeyTimestamp(timestamp: number | null) {
  if (timestamp === null) return undefined
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(timestamp * 1000))
}

function ProfileLoading() {
  return (
    <div className="grid gap-4" role="status">
      <Skeleton className="h-28 rounded-2xl" />
      <div className="grid gap-4 xl:grid-cols-2"><Skeleton className="h-72 rounded-2xl" /><Skeleton className="h-72 rounded-2xl" /></div>
      <Skeleton className="h-56 rounded-2xl" />
    </div>
  )
}

function ErrorPanel({ onRetry }: { onRetry: () => void }) {
  const { t } = useTranslation()
  return (
    <div role="alert" className="rounded-2xl border border-destructive/25 bg-destructive/8 p-5">
      <h2 className="text-sm font-semibold text-destructive">{t('profile.errors.loadTitle')}</h2>
      <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.errors.loadBody')}</p>
      <Button type="button" size="sm" variant="bordered" className="mt-4" onClick={onRetry}>
        <RefreshCw className="size-4" aria-hidden="true" />{t('profile.actions.retry')}
      </Button>
    </div>
  )
}

function ProfileCard({ profile }: { profile: UserProfileResponse }) {
  const { t } = useTranslation()
  const updateMutation = useUpdateUserProfile()
  const sendEmailMutation = useSendUserEmailBindingVerification()
  const confirmEmailMutation = useConfirmUserEmailBinding()
  const [username, setUsername] = useState(profile.username)
  const [email, setEmail] = useState(profile.email ?? '')
  const [verificationCode, setVerificationCode] = useState('')
  const [nextSendAt, setNextSendAt] = useState(0)
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))

  useEffect(() => setUsername(profile.username), [profile.username])
  useEffect(() => setEmail(profile.email ?? ''), [profile.email])
  useEffect(() => {
    if (nextSendAt <= now) return undefined
    const timer = window.setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000)
    return () => window.clearInterval(timer)
  }, [nextSendAt, now])

  const usernameValid = isValidProfileUsername(username)
  const changed = username !== profile.username
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!usernameValid || !changed) return
    try {
      await updateMutation.mutateAsync({ username })
    } catch {
      // 错误留在当前卡片，输入内容保留以便修正后重试。
    }
  }

  const error = errorCode(updateMutation.error)
  const emailError = errorCode(confirmEmailMutation.error ?? sendEmailMutation.error)
  const emailValid = /^[^\s@]+@[^\s@]+\.[^\s@]+$/u.test(email)
  const cooldownSeconds = Math.max(0, nextSendAt - now)
  const sendEmail = async () => {
    if (!emailValid || cooldownSeconds > 0 || sendEmailMutation.isPending) return
    try {
      const response = await sendEmailMutation.mutateAsync({ email })
      setNextSendAt(response.next_send_at)
      setNow(Math.floor(Date.now() / 1000))
    } catch {
      // 错误在邮箱绑定区域展示。
    }
  }
  const confirmEmail = async () => {
    if (!emailValid || !/^\d{6}$/u.test(verificationCode) || confirmEmailMutation.isPending) return
    try {
      await confirmEmailMutation.mutateAsync({ email, verification_code: verificationCode })
      setVerificationCode('')
      setNextSendAt(0)
    } catch {
      // 错误在邮箱绑定区域展示。
    }
  }
  return (
    <Card shadow="none" className="overflow-hidden border border-[var(--hairline)]">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex items-start justify-between gap-3">
          <div className="flex gap-3">
            <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-info/10 text-info"><UserRound className="size-4" aria-hidden="true" /></div>
            <div><h3 className="text-base font-semibold">{t('profile.profile.title')}</h3><p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.profile.subtitle')}</p></div>
          </div>
          <Chip className={profile.role === 'admin' ? 'border-info/25 bg-info/10 text-info' : 'bg-surface-2 text-muted-foreground'}>
            {t(`auth.account.${profile.role}`)}
          </Chip>
        </div>
      </CardHeader>
      <CardBody className="p-5">
        <form className="grid gap-4" onSubmit={submit} noValidate>
          <div className="grid gap-2">
            <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-username">{t('profile.profile.username')}</label>
            <Input id="profile-username" value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" isInvalid={username.length > 0 && !usernameValid} />
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.profile.usernameHint')}</p>
          </div>
          <div className="grid gap-2">
            <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-email">{t('profile.profile.email')}</label>
            <div className="relative">
              <Mail className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
              <Input classNames={{ input: 'pl-9' }} id="profile-email" type="email" placeholder={t('profile.profile.emailEmpty')} size="sm" value={email} onChange={(event) => { setEmail(event.target.value); confirmEmailMutation.reset(); sendEmailMutation.reset() }} />
            </div>
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.profile.emailHint')}</p>
            <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
              <Input id="profile-email-code" inputMode="numeric" maxLength={6} placeholder={t('profile.profile.emailBinding.codePlaceholder')} size="sm" value={verificationCode} onChange={(event) => setVerificationCode(event.target.value.replace(/\D/gu, '').slice(0, 6))} />
              <Button type="button" size="sm" variant="bordered" isDisabled={!emailValid || cooldownSeconds > 0 || sendEmailMutation.isPending} onPress={() => void sendEmail()}>
                {sendEmailMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{cooldownSeconds > 0 ? t('profile.profile.emailBinding.resendIn', { seconds: cooldownSeconds }) : t('profile.profile.emailBinding.sendCode')}
              </Button>
            </div>
            <div className="flex items-center justify-between gap-2">
              <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.profile.emailBinding.hint')}</p>
              <Button type="button" size="sm" color="primary" isDisabled={!emailValid || !/^\d{6}$/u.test(verificationCode) || confirmEmailMutation.isPending} onPress={() => void confirmEmail()}>
                {confirmEmailMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t('profile.profile.emailBinding.confirm')}
              </Button>
            </div>
            {confirmEmailMutation.isSuccess ? <p role="status" className="text-xs text-success">{t('profile.profile.emailBinding.success')}</p> : null}
            {emailError ? <p role="alert" className="text-xs text-destructive">{t(`profile.profile.emailBinding.errors.${emailError === 'email_not_configured' ? 'notConfigured' : emailError === 'email_delivery_failed' ? 'deliveryFailed' : emailError === 'registration_rate_limited' ? 'rateLimited' : emailError === 'registration_rejected' ? 'rejected' : emailError === 'user_conflict' ? 'conflict' : 'generic'}`)}</p> : null}
          </div>
          {updateMutation.isSuccess ? <p role="status" className="flex items-center gap-1.5 text-xs text-success"><Check className="size-3.5" aria-hidden="true" />{t('profile.profile.saved')}</p> : null}
          {updateMutation.isError ? <p role="alert" className="text-xs text-destructive">{t(error === 'user_conflict' ? 'profile.errors.conflict' : 'profile.errors.save')}</p> : null}
          <div className="flex justify-end"><Button type="submit" size="sm" color="primary" isDisabled={!changed || !usernameValid || updateMutation.isPending}>{updateMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t('profile.actions.saveProfile')}</Button></div>
        </form>
      </CardBody>
    </Card>
  )
}

function PasswordCard({ onPasswordChanged }: ProfilePageProps) {
  const { t } = useTranslation()
  const mutation = useChangeUserPassword()
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmation, setConfirmation] = useState('')
  const valid = currentPassword.length > 0 && isValidProfilePassword(newPassword) && newPassword === confirmation

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!valid) return
    try {
      await mutation.mutateAsync({ current_password: currentPassword, new_password: newPassword })
      setCurrentPassword('')
      setNewPassword('')
      setConfirmation('')
      onPasswordChanged()
    } catch {
      // 服务端返回的稳定错误码在卡片内呈现，密码输入不写入持久化缓存。
    }
  }

  const code = errorCode(mutation.error)
  return (
    <Card shadow="none" className="overflow-hidden border border-[var(--hairline)]">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex gap-3">
          <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-warning/10 text-warning"><KeyRound className="size-4" aria-hidden="true" /></div>
          <div><h3 className="text-base font-semibold">{t('profile.password.title')}</h3><p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.password.subtitle')}</p></div>
        </div>
      </CardHeader>
      <CardBody className="p-5">
        <form className="grid gap-4" onSubmit={submit} noValidate>
          <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-current-password">{t('profile.password.current')}</label><Input autoComplete="current-password" id="profile-current-password" size="sm" type="password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} /></div>
          <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-new-password">{t('profile.password.next')}</label><Input autoComplete="new-password" id="profile-new-password" isInvalid={newPassword.length > 0 && !isValidProfilePassword(newPassword)} size="sm" type="password" value={newPassword} onChange={(event) => setNewPassword(event.target.value)} /><p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.password.hint')}</p></div>
          <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-confirm-password">{t('profile.password.confirm')}</label><Input autoComplete="new-password" id="profile-confirm-password" isInvalid={confirmation.length > 0 && confirmation !== newPassword} size="sm" type="password" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} /></div>
          {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t(code === 'password_change_rejected' ? 'profile.errors.currentPassword' : 'profile.errors.password')}</p> : null}
          <div className="flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/6 p-3 text-[0.6875rem] leading-4 text-muted-foreground"><ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-warning" aria-hidden="true" /><span>{t('profile.password.revokesSessions')}</span></div>
          <div className="flex justify-end"><Button type="submit" size="sm" color="primary" isDisabled={!valid || mutation.isPending}>{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t('profile.actions.changePassword')}</Button></div>
        </form>
      </CardBody>
    </Card>
  )
}

type TwoFactorCopyTarget = 'secret' | 'uri' | 'backup'

function TwoFactorCard() {
  const { t } = useTranslation()
  const statusQuery = useUserTwoFactor()
  const enableMutation = useEnableUserTwoFactor()
  const disableMutation = useDisableUserTwoFactor()
  const [currentPassword, setCurrentPassword] = useState('')
  const [enrollment, setEnrollment] = useState<UserTwoFactorEnrollmentResponse>()
  const [copied, setCopied] = useState<TwoFactorCopyTarget>()
  const [copyFailed, setCopyFailed] = useState<TwoFactorCopyTarget>()

  const enabled = statusQuery.data?.enabled === true
  const mutation = enabled ? disableMutation : enableMutation
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!currentPassword || mutation.isPending) return
    try {
      if (enabled) {
        await disableMutation.mutateAsync({ current_password: currentPassword })
        setEnrollment(undefined)
      } else {
        const response = await enableMutation.mutateAsync({ current_password: currentPassword })
        setEnrollment(response)
      }
      setCurrentPassword('')
    } catch {
      // 保留密码输入以便用户修正后重试，但不把密码写入持久化缓存。
    }
  }

  const copy = async (target: TwoFactorCopyTarget, value: string) => {
    const success = await copyText(value)
    setCopied(success ? target : undefined)
    setCopyFailed(success ? undefined : target)
  }

  return (
    <Card shadow="none" className="overflow-hidden border border-[var(--hairline)]">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex items-start justify-between gap-3">
          <div className="flex gap-3">
            <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-success/10 text-success"><ShieldCheck className="size-4" aria-hidden="true" /></div>
            <div><h3 className="text-base font-semibold">{t('profile.twoFactor.title')}</h3><p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.twoFactor.subtitle')}</p></div>
          </div>
          {statusQuery.data ? <Chip className={enabled ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground'}>{t(enabled ? 'profile.twoFactor.enabled' : 'profile.twoFactor.disabled')}</Chip> : null}
        </div>
      </CardHeader>
      <CardBody className="p-5">
        {statusQuery.isPending ? (
          <div className="grid gap-3" role="status"><Skeleton className="h-4 w-48" /><Skeleton className="h-11 w-full" /></div>
        ) : statusQuery.isError ? (
          <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-destructive/20 bg-destructive/8 p-3 text-xs text-destructive">
            <span>{t('profile.twoFactor.loadError')}</span>
            <Button type="button" size="sm" variant="bordered" onClick={() => void statusQuery.refetch()}><RefreshCw className="size-4" aria-hidden="true" />{t('profile.actions.retry')}</Button>
          </div>
        ) : (
          <div className="grid gap-5">
            {enabled && enrollment ? <EnrollmentMaterials enrollment={enrollment} copied={copied} copyFailed={copyFailed} onCopy={copy} /> : null}
            <div className="flex items-start gap-2 rounded-xl border border-info/20 bg-info/6 p-3 text-xs leading-5 text-muted-foreground">
              {enabled ? <ShieldOff className="mt-0.5 size-3.5 shrink-0 text-info" aria-hidden="true" /> : <ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-info" aria-hidden="true" />}
              <span>{t(enabled ? 'profile.twoFactor.disableHint' : 'profile.twoFactor.enableHint')}</span>
            </div>
            <form className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end" onSubmit={submit} noValidate>
              <div className="grid gap-2">
                <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-two-factor-password">{t('profile.twoFactor.currentPassword')}</label>
                <Input autoComplete="current-password" id="profile-two-factor-password" isInvalid={mutation.isError ? true : undefined} size="sm" type="password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} />
              </div>
              <Button type="submit" size="sm" color="primary" isDisabled={!currentPassword || mutation.isPending} variant={enabled ? 'bordered' : 'solid'}>
                {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
                {mutation.isPending ? t('profile.twoFactor.working') : t(enabled ? 'profile.twoFactor.disable' : 'profile.twoFactor.enable')}
              </Button>
            </form>
            {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t(errorCode(mutation.error) === 'password_change_rejected' ? 'profile.errors.currentPassword' : errorCode(mutation.error) === 'two_factor_already_enabled' ? 'profile.twoFactor.alreadyEnabled' : 'profile.twoFactor.mutationError')}</p> : null}
          </div>
        )}
      </CardBody>
    </Card>
  )
}

function EnrollmentMaterials({ enrollment, copied, copyFailed, onCopy }: { enrollment: UserTwoFactorEnrollmentResponse; copied?: TwoFactorCopyTarget; copyFailed?: TwoFactorCopyTarget; onCopy: (target: TwoFactorCopyTarget, value: string) => Promise<void> }) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-4 rounded-xl border border-warning/25 bg-warning/6 p-4">
      <div>
        <p className="text-sm font-semibold text-warning">{t('profile.twoFactor.materialsTitle')}</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.twoFactor.materialsHint')}</p>
      </div>
      <CopyField label={t('profile.twoFactor.secret')} value={enrollment.secret} target="secret" copied={copied === 'secret'} copyFailed={copyFailed === 'secret'} onCopy={onCopy} />
      <CopyField label={t('profile.twoFactor.uri')} value={enrollment.otpauth_uri} target="uri" copied={copied === 'uri'} copyFailed={copyFailed === 'uri'} onCopy={onCopy} />
      <div className="grid gap-2">
        <div className="flex items-center justify-between gap-3"><label className="text-xs font-medium leading-none text-foreground">{t('profile.twoFactor.backupCodes')}</label><Button type="button" size="sm" variant="bordered" onClick={() => void onCopy('backup', enrollment.backup_codes.join('\n'))}>{copied === 'backup' ? <Check className="size-4" aria-hidden="true" /> : <Copy className="size-4" aria-hidden="true" />}{t(copied === 'backup' ? 'profile.twoFactor.copied' : 'profile.twoFactor.copyBackup')}</Button></div>
        <div className="grid grid-cols-2 gap-2 rounded-lg border border-[var(--hairline)] bg-background/60 p-3 font-mono text-xs tabular-nums sm:grid-cols-5">{enrollment.backup_codes.map((code) => <code key={code}>{code}</code>)}</div>
        {copyFailed === 'backup' ? <p role="alert" className="text-xs text-destructive">{t('profile.twoFactor.copyFailed')}</p> : null}
      </div>
    </div>
  )
}

function CopyField({ label, value, target, copied, copyFailed, onCopy }: { label: string; value: string; target: TwoFactorCopyTarget; copied: boolean; copyFailed: boolean; onCopy: (target: TwoFactorCopyTarget, value: string) => Promise<void> }) {
  const { t } = useTranslation()
  return (
    <div className="grid gap-2">
      <label className="text-xs font-medium leading-none text-foreground">{label}</label>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 break-all rounded-lg border border-[var(--hairline)] bg-background/60 px-3 py-2 text-xs leading-5">{value}</code>
        <Button isIconOnly type="button" size="sm" variant="bordered" title={t(copied ? 'profile.twoFactor.copied' : 'profile.twoFactor.copy')} aria-label={t(copied ? 'profile.twoFactor.copied' : 'profile.twoFactor.copy')} onClick={() => void onCopy(target, value)}>{copied ? <Check className="size-4" aria-hidden="true" /> : <Copy className="size-4" aria-hidden="true" />}</Button>
      </div>
      {copyFailed ? <p role="alert" className="text-xs text-destructive">{t('profile.twoFactor.copyFailed')}</p> : null}
    </div>
  )
}

type PasskeyLocalError = 'unsupported' | 'cancelled' | 'invalid_credential' | 'request' | 'invalid_name'

function PasskeyCard({ onPasswordChanged }: ProfilePageProps) {
  const { t } = useTranslation()
  const passkeysQuery = useUserPasskeys()
  const twoFactorQuery = useUserTwoFactor()
  const startMutation = useStartUserPasskeyRegistration()
  const finishMutation = useFinishUserPasskeyRegistration()
  const renameMutation = useRenameUserPasskey()
  const revokeMutation = useRevokeUserPasskey()
  const [displayName, setDisplayName] = useState('')
  const [registrationError, setRegistrationError] = useState<PasskeyLocalError>()
  const [registrationSuccess, setRegistrationSuccess] = useState(false)
  const [editingId, setEditingId] = useState<number | null>(null)
  const [editingName, setEditingName] = useState('')
  const [renameErrorId, setRenameErrorId] = useState<number | null>(null)
  const [revokeId, setRevokeId] = useState<number | null>(null)
  const [revokePassword, setRevokePassword] = useState('')
  const [revokeCode, setRevokeCode] = useState('')

  const registrationPending = startMutation.isPending || finishMutation.isPending
  const submitRegistration = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const name = displayName.trim()
    setRegistrationError(undefined)
    setRegistrationSuccess(false)
    startMutation.reset()
    finishMutation.reset()
    if (!name) {
      setRegistrationError('invalid_name')
      return
    }
    if (!supportsPasskeys()) {
      setRegistrationError('unsupported')
      return
    }
    try {
      const response = await startMutation.mutateAsync()
      const credential = await navigator.credentials.create({ publicKey: toCreationOptions(response) })
      if (!(credential instanceof PublicKeyCredential)) throw new Error('invalid-credential')
      await finishMutation.mutateAsync({
        display_name: name,
        credential: serializeRegistrationCredential(credential),
      })
      setDisplayName('')
      setRegistrationSuccess(true)
    } catch (error) {
      if (error instanceof DOMException && (error.name === 'AbortError' || error.name === 'NotAllowedError')) {
        setRegistrationError('cancelled')
      } else if (error instanceof Error && error.message === 'invalid-credential') {
        setRegistrationError('invalid_credential')
      } else if (!startMutation.error && !finishMutation.error) {
        setRegistrationError('request')
      }
    }
  }

  const beginRename = (item: UserPasskeyResponse) => {
    renameMutation.reset()
    setRenameErrorId(null)
    setEditingId(item.id)
    setEditingName(item.display_name)
  }

  const submitRename = async (event: FormEvent<HTMLFormElement>, id: number) => {
    event.preventDefault()
    const name = editingName.trim()
    if (!name) {
      setRenameErrorId(id)
      return
    }
    try {
      await renameMutation.mutateAsync({ id, display_name: name })
      setEditingId(null)
      setEditingName('')
      setRenameErrorId(null)
    } catch {
      setRenameErrorId(id)
    }
  }

  const beginRevoke = (id: number) => {
    revokeMutation.reset()
    setRevokeId(id)
    setRevokePassword('')
    setRevokeCode('')
  }

  const cancelRevoke = () => {
    revokeMutation.reset()
    setRevokeId(null)
    setRevokePassword('')
    setRevokeCode('')
  }

  const submitRevoke = async (event: FormEvent<HTMLFormElement>, id: number) => {
    event.preventDefault()
    if (!revokePassword || revokeMutation.isPending) return
    try {
      await revokeMutation.mutateAsync({
        id,
        current_password: revokePassword,
        totp_code: revokeCode.trim() || null,
      })
      onPasswordChanged()
    } catch {
      // 保留输入内容，让用户可以修正当前密码或二次验证码后重试。
    }
  }

  const registrationCode = errorCode(finishMutation.error) ?? errorCode(startMutation.error)
  const registrationErrorKey = registrationError === 'unsupported'
    ? 'profile.passkeys.errors.unsupported'
    : registrationError === 'cancelled'
      ? 'profile.passkeys.errors.cancelled'
      : registrationError === 'invalid_credential'
        ? 'profile.passkeys.errors.invalidCredential'
        : registrationError === 'invalid_name'
          ? 'profile.passkeys.errors.invalidName'
          : registrationError === 'request'
            ? 'profile.passkeys.errors.registration'
            : registrationCode
              ? 'profile.passkeys.errors.registration'
              : undefined
  const revokeCodeValue = errorCode(revokeMutation.error)
  const revokeErrorKey = revokeCodeValue === 'password_change_rejected'
    ? 'profile.passkeys.errors.currentPassword'
    : revokeCodeValue === 'two_factor_required'
      ? 'profile.passkeys.errors.twoFactorRequired'
      : revokeCodeValue === 'two_factor_invalid'
        ? 'profile.passkeys.errors.twoFactorInvalid'
        : revokeMutation.isError
          ? 'profile.passkeys.errors.revoke'
          : undefined
  const items = passkeysQuery.data?.items ?? []

  return (
    <Card shadow="none" className="overflow-hidden border border-[var(--hairline)]">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex items-start justify-between gap-3">
          <div className="flex gap-3">
            <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-brand/10 text-brand"><Fingerprint className="size-4" aria-hidden="true" /></div>
            <div><h3 className="text-base font-semibold">{t('profile.passkeys.title')}</h3><p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.passkeys.subtitle')}</p></div>
          </div>
          {passkeysQuery.data ? <Chip className="bg-info/10 text-info">{t('profile.passkeys.count', { count: items.length })}</Chip> : null}
        </div>
      </CardHeader>
      <CardBody className="grid gap-5 p-5">
        <form className="grid gap-3 rounded-xl border border-info/20 bg-info/6 p-4" onSubmit={submitRegistration} noValidate>
          <div>
            <p className="text-sm font-semibold">{t('profile.passkeys.addTitle')}</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.passkeys.addHint')}</p>
          </div>
          <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
            <div className="grid gap-2">
              <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-passkey-name">{t('profile.passkeys.displayName')}</label>
              <Input id="profile-passkey-name" maxLength={128} placeholder={t('profile.passkeys.displayNamePlaceholder')} size="sm" value={displayName} onChange={(event) => setDisplayName(event.target.value)} />
            </div>
            <Button type="submit" size="sm" color="primary" isDisabled={registrationPending}>
              {registrationPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Fingerprint aria-hidden="true" />}
              {registrationPending ? t('profile.passkeys.registering') : t('profile.passkeys.register')}
            </Button>
          </div>
          {registrationErrorKey ? <p role="alert" className="text-xs text-destructive">{t(registrationErrorKey)}</p> : null}
          {registrationSuccess ? <p role="status" className="flex items-center gap-1.5 text-xs text-success"><Check className="size-3.5" aria-hidden="true" />{t('profile.passkeys.registered')}</p> : null}
        </form>

        {passkeysQuery.isPending ? (
          <div className="grid gap-3" role="status"><Skeleton className="h-14 w-full" /><Skeleton className="h-14 w-full" /></div>
        ) : passkeysQuery.isError ? (
          <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-destructive/20 bg-destructive/8 p-3 text-xs text-destructive">
            <span>{t('profile.passkeys.loadError')}</span>
            <Button type="button" size="sm" variant="bordered" onClick={() => void passkeysQuery.refetch()}><RefreshCw className="size-4" aria-hidden="true" />{t('profile.actions.retry')}</Button>
          </div>
        ) : items.length === 0 ? (
          <div className="rounded-xl border border-dashed border-[var(--hairline)] bg-surface-1/35 p-5 text-center">
            <p className="text-sm font-medium">{t('profile.passkeys.emptyTitle')}</p>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.passkeys.emptyBody')}</p>
          </div>
        ) : (
          <div className="grid gap-3">
            <div className="flex items-center gap-2 text-xs text-muted-foreground"><Fingerprint className="size-3.5 text-brand" aria-hidden="true" />{t('profile.passkeys.listTitle')}</div>
            <div className="divide-y divide-[var(--hairline)] rounded-xl border border-[var(--hairline)]">
              {items.map((item) => {
                const revoked = item.revoked_at !== null
                const createdAt = formatPasskeyTimestamp(item.created_at)
                const lastUsedAt = formatPasskeyTimestamp(item.last_used_at)
                return (
                  <div key={item.id} className="grid gap-3 p-4 first:rounded-t-xl last:rounded-b-xl">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <div className="min-w-0 flex-1">
                        {editingId === item.id ? (
                          <form className="flex max-w-xl items-center gap-2" onSubmit={(event) => void submitRename(event, item.id)}>
                            <Input autoFocus aria-label={t('profile.passkeys.displayName')} maxLength={128} size="sm" value={editingName} onChange={(event) => setEditingName(event.target.value)} />
                            <Button isIconOnly type="submit" size="sm" variant="bordered" title={t('profile.passkeys.saveName')} aria-label={t('profile.passkeys.saveName')} isDisabled={renameMutation.isPending}><Check className="size-4" aria-hidden="true" /></Button>
                            <Button isIconOnly type="button" size="sm" variant="light" title={t('profile.passkeys.cancel')} aria-label={t('profile.passkeys.cancel')} onClick={() => { setEditingId(null); setRenameErrorId(null) }}><X className="size-4" aria-hidden="true" /></Button>
                          </form>
                        ) : (
                          <>
                            <p className="truncate text-sm font-medium">{item.display_name}</p>
                            <p className="mt-1 text-xs text-muted-foreground">
                              {createdAt ? t('profile.passkeys.createdAt', { value: createdAt }) : null}
                              {lastUsedAt ? <span className="ml-2">{t('profile.passkeys.lastUsedAt', { value: lastUsedAt })}</span> : <span className="ml-2">{t('profile.passkeys.neverUsed')}</span>}
                            </p>
                          </>
                        )}
                        {renameErrorId === item.id ? <p role="alert" className="mt-2 text-xs text-destructive">{renameMutation.isError ? t('profile.passkeys.errors.rename') : t('profile.passkeys.errors.invalidName')}</p> : null}
                      </div>
                      <div className="flex shrink-0 items-center gap-2">
                        {revoked ? <Chip className="bg-destructive/10 text-destructive">{t('profile.passkeys.revoked')}</Chip> : null}
                        {!revoked && editingId !== item.id ? <>
                          <Button isIconOnly type="button" size="sm" variant="light" title={t('profile.passkeys.rename')} aria-label={t('profile.passkeys.rename')} onClick={() => beginRename(item)}><Pencil className="size-4" aria-hidden="true" /></Button>
                          <Button type="button" size="sm" variant="bordered" onClick={() => beginRevoke(item.id)}><Trash2 className="size-4" aria-hidden="true" />{t('profile.passkeys.revoke')}</Button>
                        </> : null}
                      </div>
                    </div>
                    {revokeId === item.id ? (
                      <form className="grid gap-3 rounded-lg border border-warning/25 bg-warning/6 p-3" onSubmit={(event) => void submitRevoke(event, item.id)} noValidate>
                        <div>
                          <p className="text-sm font-medium text-warning">{t('profile.passkeys.revokeConfirmTitle')}</p>
                          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.passkeys.revokeConfirmBody')}</p>
                        </div>
                        <div className="grid gap-3 sm:grid-cols-2">
                          <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor={`profile-passkey-password-${item.id}`}>{t('profile.passkeys.currentPassword')}</label><Input autoComplete="current-password" id={`profile-passkey-password-${item.id}`} size="sm" type="password" value={revokePassword} onChange={(event) => setRevokePassword(event.target.value)} /></div>
                          <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor={`profile-passkey-code-${item.id}`}>{t('profile.passkeys.twoFactorCode')}</label><Input autoComplete="one-time-code" id={`profile-passkey-code-${item.id}`} inputMode="numeric" placeholder={twoFactorQuery.data?.enabled ? t('profile.passkeys.twoFactorRequiredPlaceholder') : t('profile.passkeys.twoFactorOptionalPlaceholder')} size="sm" value={revokeCode} onChange={(event) => setRevokeCode(event.target.value)} /><p className="text-[0.6875rem] leading-4 text-muted-foreground">{t(twoFactorQuery.data?.enabled ? 'profile.passkeys.twoFactorRequiredHint' : 'profile.passkeys.twoFactorOptionalHint')}</p></div>
                        </div>
                        {revokeErrorKey ? <p role="alert" className="text-xs text-destructive">{t(revokeErrorKey)}</p> : null}
                        <div className="flex flex-wrap justify-end gap-2"><Button type="button" size="sm" variant="light" onClick={cancelRevoke}>{t('profile.passkeys.cancel')}</Button><Button type="submit" size="sm" color="danger" variant="flat" isDisabled={!revokePassword || revokeMutation.isPending}>{revokeMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-4" aria-hidden="true" />}{revokeMutation.isPending ? t('profile.passkeys.revoking') : t('profile.passkeys.confirmRevoke')}</Button></div>
                      </form>
                    ) : null}
                  </div>
                )
              })}
            </div>
          </div>
        )}
      </CardBody>
    </Card>
  )
}

function NotificationsCard({ profile }: { profile: UserProfileResponse }) {
  const { t } = useTranslation()
  const mutation = useUpdateUserNotificationPreferences()
  const [productUpdates, setProductUpdates] = useState(profile.notifications.email_product_updates)
  const [usageAlerts, setUsageAlerts] = useState(profile.notifications.email_usage_alerts)
  const [thresholdMode, setThresholdMode] = useState<'inherit' | 'custom'>(
    profile.notifications.balance_alert_threshold === null ? 'inherit' : 'custom',
  )
  const [thresholdInput, setThresholdInput] = useState(String(
    profile.notifications.balance_alert_threshold ?? profile.notifications.effective_balance_alert_threshold,
  ))

  useEffect(() => {
    setProductUpdates(profile.notifications.email_product_updates)
    setUsageAlerts(profile.notifications.email_usage_alerts)
    setThresholdMode(profile.notifications.balance_alert_threshold === null ? 'inherit' : 'custom')
    setThresholdInput(String(
      profile.notifications.balance_alert_threshold ?? profile.notifications.effective_balance_alert_threshold,
    ))
  }, [profile.notifications])

  // HeroUI Select 不接受空字符串 key，动态选项走 items + 渲染函数。
  const thresholdModeItems = (['inherit', 'custom'] as const).map((key) => ({
    key,
    label: t(`profile.notifications.threshold${key === 'inherit' ? 'Inherit' : 'Custom'}`),
  }))
  const parsedThreshold = Number(thresholdInput)
  const thresholdValid = thresholdMode === 'inherit'
    || (Number.isSafeInteger(parsedThreshold) && parsedThreshold >= 1)
  const savedThreshold = thresholdMode === 'inherit' ? null : parsedThreshold
  const changed = productUpdates !== profile.notifications.email_product_updates
    || usageAlerts !== profile.notifications.email_usage_alerts
    || (thresholdValid && savedThreshold !== profile.notifications.balance_alert_threshold)
  const submit = async () => {
    if (!changed || !thresholdValid) return
    try {
      await mutation.mutateAsync({
        email_product_updates: productUpdates,
        email_usage_alerts: usageAlerts,
        balance_alert_threshold: savedThreshold,
      })
    } catch {
      // 保留本地选择，允许用户修复网络或服务端问题后重试。
    }
  }

  return (
    <Card shadow="none" className="overflow-hidden border border-[var(--hairline)]">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex gap-3"><div className="grid size-9 shrink-0 place-items-center rounded-xl bg-brand/10 text-brand"><BellRing className="size-4" aria-hidden="true" /></div><div><h3 className="text-base font-semibold">{t('profile.notifications.title')}</h3><p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.notifications.subtitle')}</p></div></div>
      </CardHeader>
      <CardBody className="p-5">
        <div className="divide-y divide-[var(--hairline)]">
          <PreferenceRow id="profile-product-updates" title={t('profile.notifications.productTitle')} description={t('profile.notifications.productDescription')} checked={productUpdates} onCheckedChange={setProductUpdates} />
          <PreferenceRow id="profile-usage-alerts" title={t('profile.notifications.usageTitle')} description={t('profile.notifications.usageDescription')} checked={usageAlerts} onCheckedChange={setUsageAlerts} />
          <div className="py-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="flex min-w-0 gap-3">
                <CircleDollarSign className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
                <div>
                  <p className="text-sm font-medium">{t('profile.notifications.balanceAlertTitle')}</p>
                  <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.notifications.balanceAlertDescription')}</p>
                </div>
              </div>
              <Chip className={profile.notifications.balance_alert_enabled ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'}>
                {t(profile.notifications.balance_alert_enabled
                  ? 'profile.notifications.balanceAlertActive'
                  : 'profile.notifications.balanceAlertPaused')}
              </Chip>
            </div>
            <div className="mt-4 grid gap-3 sm:grid-cols-2">
              <div className="grid gap-2">
                <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-balance-alert-mode">{t('profile.notifications.thresholdMode')}</label>
                <Select
                  aria-label={t('profile.notifications.thresholdMode')}
                  id="profile-balance-alert-mode"
                  items={thresholdModeItems}
                  selectedKeys={[thresholdMode]}
                  size="sm"
                  onSelectionChange={(keys) => setThresholdMode(String(Array.from(keys)[0] ?? 'inherit') === 'custom' ? 'custom' : 'inherit')}
                >
                  {({ key, label }) => <SelectItem key={key}>{label}</SelectItem>}
                </Select>
              </div>
              <div className="grid gap-2">
                <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-balance-alert-threshold">{t('profile.notifications.thresholdValue')}</label>
                <div className="relative">
                  <Input
                    className="tabular-nums"
                    id="profile-balance-alert-threshold"
                    isDisabled={thresholdMode === 'inherit'}
                    isInvalid={!thresholdValid}
                    min={1}
                    size="sm"
                    step={1}
                    type="number"
                    value={thresholdInput}
                    onChange={(event) => setThresholdInput(event.target.value)}
                  />
                  <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 pr-12 text-xs text-muted-foreground">
                    {t('profile.notifications.quotaUnit')}
                  </span>
                </div>
                {!thresholdValid ? <p className="text-xs text-destructive">{t('profile.notifications.thresholdInvalid')}</p> : null}
              </div>
            </div>
            <div className="mt-3 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <span>{t('profile.notifications.effectiveThreshold')}</span>
              <Chip className="bg-info/10 font-mono text-info">
                {thresholdMode === 'custom' && thresholdValid
                  ? parsedThreshold
                  : profile.notifications.effective_balance_alert_threshold}
              </Chip>
            </div>
          </div>
          <div className="flex flex-wrap items-start justify-between gap-3 py-4">
            <div className="flex min-w-0 gap-3">
              <CalendarClock className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
              <div>
                <p className="text-sm font-medium">{t('profile.notifications.subscriptionAlertTitle')}</p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">
                  {t('profile.notifications.subscriptionAlertDescription', {
                    percent: profile.notifications.subscription_remaining_percent,
                  })}
                </p>
              </div>
            </div>
            <Chip className={profile.notifications.subscription_alert_enabled ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'}>
              {t(profile.notifications.subscription_alert_enabled
                ? 'profile.notifications.subscriptionAlertActive'
                : 'profile.notifications.subscriptionAlertPaused')}
            </Chip>
          </div>
          <div className="flex items-start gap-3 py-4"><ShieldCheck className="mt-0.5 size-4 shrink-0 text-success" aria-hidden="true" /><div className="min-w-0"><p className="text-sm font-medium">{t('profile.notifications.securityTitle')}</p><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.notifications.securityDescription')}</p></div><Chip className="ml-auto shrink-0 bg-success/10 text-success">{t('profile.notifications.alwaysOn')}</Chip></div>
        </div>
        <Divider className="my-4" />
        <div className="flex flex-wrap items-center justify-between gap-3"><div className="text-xs text-muted-foreground">{mutation.isSuccess ? <span className="flex items-center gap-1.5 text-success"><Check className="size-3.5" aria-hidden="true" />{t('profile.notifications.saved')}</span> : mutation.isError ? <span className="text-destructive">{t('profile.errors.notifications')}</span> : t('profile.notifications.footer')}</div><Button type="button" size="sm" color="primary" isDisabled={!changed || !thresholdValid || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t('profile.actions.saveNotifications')}</Button></div>
      </CardBody>
    </Card>
  )
}

function NotificationHistoryCard() {
  const { t, i18n } = useTranslation()
  const query = useUserNotifications()
  const markReadMutation = useMarkUserNotificationsRead()
  const [pendingId, setPendingId] = useState<number>()
  const formatTime = (value: number) => new Intl.DateTimeFormat(
    i18n.resolvedLanguage?.startsWith('zh') ? 'zh-CN' : 'en-US',
    { dateStyle: 'medium', timeStyle: 'short' },
  ).format(new Date(value * 1000))
  const entries = query.data?.pages.flatMap((page) => page.entries) ?? []
  const unreadCount = query.data?.pages[0]?.unread_count
  const markRead = async (id: number) => {
    if (markReadMutation.isPending) return
    setPendingId(id)
    try {
      await markReadMutation.mutateAsync([id])
    } finally {
      setPendingId(undefined)
    }
  }

  return (
    <Card shadow="none" className="overflow-hidden border border-[var(--hairline)]">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45 p-5">
        <div className="flex gap-3">
          <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-info/10 text-info"><BellRing className="size-4" aria-hidden="true" /></div>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-base font-semibold">{t('profile.notificationHistory.title')}</h3>
              {unreadCount !== undefined ? <Chip className={unreadCount > 0 ? 'bg-warning/10 text-warning' : 'bg-muted text-muted-foreground'}>{t('profile.notificationHistory.unreadCount', { count: unreadCount })}</Chip> : null}
            </div>
            <p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.notificationHistory.subtitle')}</p>
          </div>
        </div>
      </CardHeader>
      <CardBody className="p-0">
        {query.isPending ? <div className="grid gap-3 p-5"><Skeleton className="h-12" /><Skeleton className="h-12" /></div> : null}
        {query.isError ? <div role="alert" className="flex items-center justify-between gap-3 p-5 text-xs text-destructive"><span>{t('profile.notificationHistory.loadError')}</span><Button type="button" size="sm" variant="bordered" onClick={() => void query.refetch()}>{t('profile.actions.retry')}</Button></div> : null}
        {!query.isPending && !query.isError && entries.length === 0 ? <div className="p-5 text-xs text-muted-foreground">{t('profile.notificationHistory.empty')}</div> : null}
        {markReadMutation.isError ? <div role="alert" className="border-t border-[var(--hairline)] px-5 py-3 text-xs text-destructive">{t('profile.notificationHistory.markReadError')}</div> : null}
        {entries.length > 0 ? <div className="divide-y divide-[var(--hairline)]">{entries.map((entry) => <NotificationHistoryRow key={entry.id} entry={entry} formatTime={formatTime} onMarkRead={markRead} pending={pendingId === entry.id && markReadMutation.isPending} />)}</div> : null}
        {query.isFetchNextPageError ? <div role="alert" className="flex items-center justify-between gap-3 border-t border-[var(--hairline)] p-4 text-xs text-destructive"><span>{t('profile.notificationHistory.olderLoadError')}</span><Button type="button" size="sm" variant="light" onClick={() => void query.fetchNextPage()}>{t('profile.actions.retry')}</Button></div> : null}
        {query.hasNextPage && !query.isFetchNextPageError ? <div className="flex justify-center border-t border-[var(--hairline)] p-4"><Button type="button" size="sm" variant="bordered" isDisabled={query.isFetchingNextPage} onClick={() => void query.fetchNextPage()}>{t(query.isFetchingNextPage ? 'profile.notificationHistory.loading' : 'profile.notificationHistory.loadOlder')}</Button></div> : null}
      </CardBody>
    </Card>
  )
}

function NotificationHistoryRow({ entry, formatTime, onMarkRead, pending }: { entry: UserNotification; formatTime: (value: number) => string; onMarkRead: (id: number) => void; pending: boolean }) {
  const { t, i18n } = useTranslation()
  const isRead = entry.read_at !== undefined && entry.read_at !== null
  const title = t(`profile.notificationHistory.kind.${entry.kind}`)
  const state = t(`profile.notificationHistory.state.${entry.delivery_state}`)
  const announcementTitle = i18n.resolvedLanguage?.startsWith('zh')
    ? entry.announcement_title_zh ?? entry.announcement_title_en
    : entry.announcement_title_en ?? entry.announcement_title_zh
  const announcementBody = i18n.resolvedLanguage?.startsWith('zh')
    ? entry.announcement_body_zh ?? entry.announcement_body_en
    : entry.announcement_body_en ?? entry.announcement_body_zh
  const announcementState = entry.announcement_status === 3
    ? t('profile.notificationHistory.announcementStatus.revoked')
    : entry.announcement_visible_until !== null
      && entry.announcement_visible_until !== undefined
      && entry.announcement_visible_until <= Math.floor(Date.now() / 1000)
      ? t('profile.notificationHistory.announcementStatus.expired')
      : t('profile.notificationHistory.announcementStatus.published')
  const detail = entry.kind === 'product_update'
    ? `${announcementBody ?? t('profile.notificationHistory.productUpdateDetail')} · ${announcementState}`
    : entry.kind === 'balance_alert'
    ? t('profile.notificationHistory.balanceDetail', { observed: entry.observed_quota ?? '--', threshold: entry.threshold_quota ?? '--' })
    : entry.kind === 'subscription_purchase'
      ? t('profile.notificationHistory.purchaseDetail')
    : t('profile.notificationHistory.subscriptionDetail', { used: entry.quota_used ?? '--', amount: entry.quota_amount ?? '--', threshold: entry.threshold_percent ?? '--' })

  return <article className="grid gap-3 px-5 py-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className={isRead ? 'text-sm font-medium' : 'text-sm font-semibold'}>{entry.kind === 'product_update' && announcementTitle ? announcementTitle : title}</span><Chip className={isRead ? 'bg-muted text-muted-foreground' : 'bg-brand/10 text-brand'}>{t(`profile.notificationHistory.readState.${isRead ? 'read' : 'unread'}`)}</Chip><Chip className={entry.delivery_state === 'accepted' || entry.delivery_state === 'available' ? 'bg-success/10 text-success' : entry.delivery_state === 'failed' ? 'bg-destructive/10 text-destructive' : 'bg-warning/10 text-warning'}>{state}</Chip></div><p className="mt-1 text-xs text-muted-foreground">{detail}</p><p className="mt-1 text-[0.6875rem] text-muted-foreground/75">{t('profile.notificationHistory.meta', { time: formatTime(entry.occurred_at), attempts: entry.delivery_attempts, id: entry.id })}</p></div><div className="flex items-center justify-between gap-3 sm:flex-col sm:items-end"><span className="text-[0.6875rem] text-muted-foreground">{t(`profile.notificationHistory.channel.${entry.channel}`)}</span>{isRead ? <span className="text-[0.6875rem] text-muted-foreground">{t('profile.notificationHistory.readAt', { time: formatTime(entry.read_at ?? entry.occurred_at) })}</span> : <Button type="button" size="sm" variant="bordered" isDisabled={pending} onClick={() => onMarkRead(entry.id)}>{pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Check className="size-4" aria-hidden="true" />}{t('profile.notificationHistory.markRead')}</Button>}</div></article>
}

function PreferenceRow({ id, title, description, checked, onCheckedChange }: { id: string; title: string; description: string; checked: boolean; onCheckedChange: (checked: boolean) => void }) {
  return (
    <div className="flex items-start gap-3 py-4 first:pt-0 last:pb-0"><div className="min-w-0 flex-1"><label className="text-sm font-medium" htmlFor={id}>{title}</label><p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p></div><Switch aria-label={title} id={id} isSelected={checked} size="sm" onValueChange={onCheckedChange} /></div>
  )
}

/** 组合当前用户的资料、密码和通知偏好设置，并保持敏感输入只存在于内存。 */
export function ProfilePage({ section, onPasswordChanged }: ProfilePageProps) {
  const { t } = useTranslation()
  const profileQuery = useUserProfile()
  const notificationsRef = useRef<HTMLDivElement>(null)
  const profile = profileQuery.data
  const roleLabel = useMemo(() => profile ? t(`auth.account.${profile.role}`) : undefined, [profile, t])

  useEffect(() => {
    if (section !== 'notifications' || !profile || profileQuery.isPending) return
    const frame = requestAnimationFrame(() => {
      const target = notificationsRef.current
      if (!target) return
      target.scrollIntoView({ behavior: 'smooth', block: 'start' })
      target.focus({ preventScroll: true })
    })
    return () => cancelAnimationFrame(frame)
  }, [profile, profileQuery.isPending, section])

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div><div className="mb-1 flex items-center gap-2 text-[0.6875rem] uppercase tracking-[0.18em] text-brand"><UserRound className="size-3.5" aria-hidden="true" />{roleLabel ?? t('profile.eyebrow')}</div><h2 className="text-lg font-semibold">{t('profile.title')}</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('profile.subtitle')}</p></div>
        <Button type="button" size="sm" variant="bordered" isDisabled={profileQuery.isFetching} onClick={() => void profileQuery.refetch()}><RefreshCw className={profileQuery.isFetching ? 'size-4 animate-spin' : 'size-4'} aria-hidden="true" />{t('profile.actions.refresh')}</Button>
      </header>
      {profileQuery.isPending ? <ProfileLoading /> : profileQuery.isError || !profile ? <ErrorPanel onRetry={() => void profileQuery.refetch()} /> : (
        <div className="flex flex-col gap-8">
          <ProfileSection title={t('profile.sections.account')}>
            <div className="grid gap-4 xl:grid-cols-2"><ProfileCard profile={profile} /><PasswordCard onPasswordChanged={onPasswordChanged} /></div>
          </ProfileSection>
          <ProfileSection title={t('profile.sections.security')}>
            <div className="grid gap-4"><TwoFactorCard /><PasskeyCard onPasswordChanged={onPasswordChanged} /></div>
          </ProfileSection>
          <ProfileSection title={t('profile.sections.notifications')}>
            <div ref={notificationsRef} id="profile-notifications" tabIndex={-1} className="grid gap-4 scroll-mt-16 rounded-2xl outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
              <NotificationsCard profile={profile} />
              <NotificationHistoryCard />
            </div>
          </ProfileSection>
        </div>
      )}
    </div>
  )
}

function ProfileSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className="text-xs font-medium uppercase tracking-[0.14em] text-muted-foreground">{title}</h3>
      {children}
    </section>
  )
}
