import { Button, Card, CardBody, CardHeader, Chip, Input, Modal, ModalBody, ModalContent, ModalFooter, ModalHeader, Select, SelectItem, Skeleton, Switch } from '@heroui/react'
import { type FormEvent, type ReactNode, useEffect, useMemo, useRef, useState } from 'react'
import {
  BellRing,
  Check,
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

/*
 * 主卡片：一行横排展示头像、用户名（右侧铅笔图标改用户名）、角色与邮箱，右侧放"修改密码""修改邮箱"两个入口。
 * 三处修改都在弹窗里完成，卡面只保留当前信息。
 */
function ProfileCard({ profile, roleLabel, onPasswordChanged }: { profile: UserProfileResponse; roleLabel?: string; onPasswordChanged: () => void }) {
  const { t } = useTranslation()
  const [usernameOpen, setUsernameOpen] = useState(false)
  const [emailOpen, setEmailOpen] = useState(false)
  const [passwordOpen, setPasswordOpen] = useState(false)
  const initial = (profile.username || profile.email || '?').trim().charAt(0).toUpperCase() || '?'
  return (
    <>
      <section className="flex flex-col gap-4 py-2 sm:flex-row sm:items-center sm:justify-between" aria-label={t('profile.sections.account')}>
        <div className="flex min-w-0 items-center gap-4">
          <div className="grid size-16 shrink-0 place-items-center rounded-2xl bg-brand/10 text-xl font-semibold text-brand" aria-hidden="true">{initial}</div>
          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-2">
              <h2 className="truncate text-2xl font-semibold tracking-tight">{profile.username}</h2>
              <Button isIconOnly aria-label={t('profile.profile.editUsername')} className="shrink-0" size="sm" variant="light" onClick={() => setUsernameOpen(true)}><Pencil className="size-3.5" aria-hidden="true" /></Button>
              {roleLabel ? <Chip className={profile.role === 'admin' ? 'shrink-0 border-info/25 bg-info/10 text-info' : 'shrink-0 bg-surface-2 text-muted-foreground'} size="sm">{roleLabel}</Chip> : null}
            </div>
            <p className="mt-1 flex min-w-0 items-center gap-1.5 text-sm text-muted-foreground"><Mail className="size-3.5 shrink-0" aria-hidden="true" /><span className="truncate">{profile.email ?? t('profile.profile.emailEmpty')}</span></p>
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap gap-2">
          <Button className="gap-1.5" size="sm" variant="bordered" startContent={<KeyRound className="size-3.5" aria-hidden="true" />} onClick={() => setPasswordOpen(true)}>{t('profile.actions.changePassword')}</Button>
          <Button className="gap-1.5" size="sm" startContent={<Mail className="size-3.5" aria-hidden="true" />} onClick={() => setEmailOpen(true)}>{profile.email ? t('profile.profile.emailBinding.change') : t('profile.profile.emailBinding.bind')}</Button>
        </div>
        <Modal isOpen={usernameOpen} onOpenChange={setUsernameOpen} placement="center" size="md">
          <ModalContent>{(onClose) => <UsernameModalBody current={profile.username} onClose={onClose} />}</ModalContent>
        </Modal>
        <Modal isOpen={emailOpen} onOpenChange={setEmailOpen} placement="center" size="md">
          <ModalContent>{(onClose) => <EmailBindingBody currentEmail={profile.email} onBound={onClose} onClose={onClose} />}</ModalContent>
        </Modal>
        <Modal isOpen={passwordOpen} onOpenChange={setPasswordOpen} placement="center" size="md">
          <ModalContent>{(onClose) => <PasswordModalBody onClose={onClose} onPasswordChanged={onPasswordChanged} />}</ModalContent>
        </Modal>
      </section>
    </>
  )
}

/** 用户名弹窗：保存成功后由 updateMutation 写回资料缓存，卡面随之更新。 */
function UsernameModalBody({ current, onClose }: { current: string; onClose: () => void }) {
  const { t } = useTranslation()
  const mutation = useUpdateUserProfile()
  const [username, setUsername] = useState(current)
  const valid = isValidProfileUsername(username)
  const changed = username !== current
  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!valid || !changed || mutation.isPending) return
    try {
      await mutation.mutateAsync({ username })
      onClose()
    } catch {
      // 错误留在弹窗内，输入内容保留以便修正后重试。
    }
  }
  const code = errorCode(mutation.error)
  return (
    <>
      <ModalHeader className="flex flex-col gap-1"><span>{t('profile.profile.editUsername')}</span><span className="text-sm font-normal text-muted-foreground">{t('profile.profile.usernameHint')}</span></ModalHeader>
      <ModalBody>
        <form id="profile-username-form" className="grid gap-2" onSubmit={submit} noValidate>
          <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-username">{t('profile.profile.username')}</label>
          <Input autoComplete="username" id="profile-username" isInvalid={username.length > 0 && !valid} value={username} onChange={(event) => setUsername(event.target.value)} />
          {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t(code === 'user_conflict' ? 'profile.errors.conflict' : 'profile.errors.save')}</p> : null}
        </form>
      </ModalBody>
      <ModalFooter>
        <Button size="sm" variant="light" isDisabled={mutation.isPending} onClick={onClose}>{t('profile.actions.cancel')}</Button>
        <Button size="sm" color="primary" form="profile-username-form" isDisabled={!changed || !valid || mutation.isPending} type="submit">{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t('profile.actions.saveProfile')}</Button>
      </ModalFooter>
    </>
  )
}

function PasswordModalBody({ onClose, onPasswordChanged }: { onClose: () => void; onPasswordChanged: () => void }) {
  const { t } = useTranslation()
  const mutation = useChangeUserPassword()
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [confirmation, setConfirmation] = useState('')
  const valid = currentPassword.length > 0 && isValidProfilePassword(newPassword) && newPassword === confirmation

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!valid || mutation.isPending) return
    try {
      await mutation.mutateAsync({ current_password: currentPassword, new_password: newPassword })
      onPasswordChanged()
      onClose()
    } catch {
      // 服务端返回的稳定错误码在弹窗内呈现，密码输入不写入持久化缓存。
    }
  }

  const code = errorCode(mutation.error)
  return (
    <>
      <ModalHeader className="flex flex-col gap-1"><span>{t('profile.password.title')}</span><span className="text-sm font-normal text-muted-foreground">{t('profile.password.subtitle')}</span></ModalHeader>
      <ModalBody>
        <form id="profile-password-form" className="grid gap-4" onSubmit={submit} noValidate>
          <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-current-password">{t('profile.password.current')}</label><Input autoComplete="current-password" id="profile-current-password" size="sm" type="password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} /></div>
          <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-new-password">{t('profile.password.next')}</label><Input autoComplete="new-password" id="profile-new-password" isInvalid={newPassword.length > 0 && !isValidProfilePassword(newPassword)} size="sm" type="password" value={newPassword} onChange={(event) => setNewPassword(event.target.value)} /><p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.password.hint')}</p></div>
          <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-confirm-password">{t('profile.password.confirm')}</label><Input autoComplete="new-password" id="profile-confirm-password" isInvalid={confirmation.length > 0 && confirmation !== newPassword} size="sm" type="password" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} /></div>
          {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t(code === 'password_change_rejected' ? 'profile.errors.currentPassword' : 'profile.errors.password')}</p> : null}
          <div className="flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/6 p-3 text-[0.6875rem] leading-4 text-muted-foreground"><ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-warning" aria-hidden="true" /><span>{t('profile.password.revokesSessions')}</span></div>
        </form>
      </ModalBody>
      <ModalFooter>
        <Button size="sm" variant="light" isDisabled={mutation.isPending} onClick={onClose}>{t('profile.actions.cancel')}</Button>
        <Button size="sm" color="primary" form="profile-password-form" isDisabled={!valid || mutation.isPending} type="submit">{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t('profile.actions.changePassword')}</Button>
      </ModalFooter>
    </>
  )
}

type TwoFactorCopyTarget = 'secret' | 'uri' | 'backup'
type TwoFactorMaterials =
  | { kind: 'setup'; secret: string; uri: string; expiresAt: number }
  | { kind: 'backup'; codes: string[] }

function EnrollmentQr({ value, title }: { value: string; title: string }) {
  const { t } = useTranslation()
  const [Qr, setQr] = useState<typeof import('@rc-component/qrcode').QRCodeSVG>()
  const [failed, setFailed] = useState(false)
  const [attempt, setAttempt] = useState(0)
  useEffect(() => {
    let cancelled = false
    void import('@rc-component/qrcode')
      .then((module) => { if (!cancelled) setQr(() => module.QRCodeSVG) })
      .catch(() => { if (!cancelled) setFailed(true) })
    return () => { cancelled = true }
  }, [attempt])
  return (
    <div className="flex min-h-48 w-48 items-center justify-center rounded-lg bg-white p-3">
      {Qr ? <Qr value={value} size={168} title={title} /> : failed ? (
        <div className="grid gap-3 text-center text-xs text-black">
          <p role="alert">{t('profile.twoFactor.qrError')}</p>
          <Button type="button" size="sm" variant="bordered" onClick={() => { setFailed(false); setAttempt((value) => value + 1) }}>{t('profile.actions.retry')}</Button>
        </div>
      ) : <span role="status" className="text-xs text-black">{t('profile.twoFactor.qrLoading')}</span>}
    </div>
  )
}

function EmailBindingBody({ currentEmail, onClose, onBound }: { currentEmail?: string | null; onClose: () => void; onBound: () => void }) {
  const { t } = useTranslation()
  const sendEmailMutation = useSendUserEmailBindingVerification()
  const confirmEmailMutation = useConfirmUserEmailBinding()
  const [email, setEmail] = useState(currentEmail ?? '')
  const [verificationCode, setVerificationCode] = useState('')
  const [nextSendAt, setNextSendAt] = useState(0)
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))
  useEffect(() => {
    if (nextSendAt <= now) return undefined
    const timer = window.setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000)
    return () => window.clearInterval(timer)
  }, [nextSendAt, now])

  const emailValid = /^[^\s@]+@[^\s@]+\.[^\s@]+$/u.test(email)
  const cooldownSeconds = Math.max(0, nextSendAt - now)
  const sendEmail = async () => {
    if (!emailValid || cooldownSeconds > 0 || sendEmailMutation.isPending) return
    try {
      const response = await sendEmailMutation.mutateAsync({ email })
      setNextSendAt(response.next_send_at)
      setNow(Math.floor(Date.now() / 1000))
    } catch {
      // 错误在弹窗内展示。
    }
  }
  const confirmEmail = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!emailValid || !/^\d{6}$/u.test(verificationCode) || confirmEmailMutation.isPending) return
    try {
      await confirmEmailMutation.mutateAsync({ email, verification_code: verificationCode })
      onBound()
      onClose()
    } catch {
      // 错误在弹窗内展示。
    }
  }
  const emailError = errorCode(confirmEmailMutation.error ?? sendEmailMutation.error)
  return (
    <>
      <ModalHeader className="flex flex-col gap-1"><span>{currentEmail ? t('profile.profile.emailBinding.changeTitle') : t('profile.profile.emailBinding.bindTitle')}</span><span className="text-sm font-normal text-muted-foreground">{t('profile.profile.emailHint')}</span></ModalHeader>
      <ModalBody>
        <form id="profile-email-form" className="grid gap-4" onSubmit={(event) => void confirmEmail(event)} noValidate>
          <div className="grid gap-2">
            <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-email">{t('profile.profile.email')}</label>
            <Input id="profile-email" type="email" autoComplete="email" placeholder={t('profile.profile.emailEmpty')} size="sm" value={email} onChange={(event) => { setEmail(event.target.value); confirmEmailMutation.reset(); sendEmailMutation.reset() }} />
          </div>
          <div className="grid gap-2">
            <span className="text-xs font-medium leading-none text-foreground">{t('profile.profile.emailBinding.codePlaceholder')}</span>
            <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
              <Input id="profile-email-code" aria-label={t('profile.profile.emailBinding.codePlaceholder')} inputMode="numeric" maxLength={6} placeholder={t('profile.profile.emailBinding.codePlaceholder')} size="sm" value={verificationCode} onChange={(event) => setVerificationCode(event.target.value.replace(/\D/gu, '').slice(0, 6))} />
              <Button type="button" size="sm" variant="bordered" isDisabled={!emailValid || email === currentEmail || cooldownSeconds > 0 || sendEmailMutation.isPending} onPress={() => void sendEmail()}>
                {sendEmailMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{cooldownSeconds > 0 ? t('profile.profile.emailBinding.resendIn', { seconds: cooldownSeconds }) : t('profile.profile.emailBinding.sendCode')}
              </Button>
            </div>
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{email === currentEmail ? t('profile.profile.emailBinding.sameHint') : t('profile.profile.emailBinding.hint')}</p>
          </div>
          {emailError ? <p role="alert" className="text-xs text-destructive">{t(`profile.profile.emailBinding.errors.${emailError === 'email_not_configured' ? 'notConfigured' : emailError === 'email_delivery_failed' ? 'deliveryFailed' : emailError === 'registration_rate_limited' ? 'rateLimited' : emailError === 'registration_rejected' ? 'rejected' : emailError === 'user_conflict' ? 'conflict' : 'generic'}`)}</p> : null}
        </form>
      </ModalBody>
      <ModalFooter>
        <Button size="sm" variant="light" isDisabled={confirmEmailMutation.isPending} onClick={onClose}>{t('profile.actions.cancel')}</Button>
        <Button size="sm" color="primary" form="profile-email-form" type="submit" isDisabled={!emailValid || email === currentEmail || !/^\d{6}$/u.test(verificationCode) || confirmEmailMutation.isPending}>
          {confirmEmailMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t('profile.profile.emailBinding.confirm')}
        </Button>
      </ModalFooter>
    </>
  )
}

function TwoFactorCard() {
  const { t } = useTranslation()
  const statusQuery = useUserTwoFactor()
  const enableMutation = useEnableUserTwoFactor()
  const disableMutation = useDisableUserTwoFactor()
  const [currentPassword, setCurrentPassword] = useState('')
  const [totpCode, setTotpCode] = useState('')
  const [materials, setMaterials] = useState<TwoFactorMaterials>()
  const [copied, setCopied] = useState<TwoFactorCopyTarget>()
  const [copyFailed, setCopyFailed] = useState<TwoFactorCopyTarget>()
  const [formError, setFormError] = useState<string>()
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))
  const mounted = useRef(true)
  const resetEnable = enableMutation.reset
  const resetDisable = disableMutation.reset

  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      resetEnable()
      resetDisable()
    }
  }, [resetEnable, resetDisable])

  const status = statusQuery.data
  const enabled = status?.enabled === true
  const pendingExpiresAt = status?.pending_expires_at ?? null
  const expired = !enabled && status?.state === 'pending' && pendingExpiresAt !== null && pendingExpiresAt <= now
  const pending = !enabled && status?.state === 'pending' && !expired
  const working = enableMutation.isPending || disableMutation.isPending
  const setupMaterials = pending && materials?.kind === 'setup' && materials.expiresAt === pendingExpiresAt ? materials : undefined
  const backupMaterials = enabled && materials?.kind === 'backup' ? materials : undefined

  useEffect(() => {
    if (enabled || pendingExpiresAt === null) return undefined
    const timer = window.setInterval(() => {
      const timestamp = Math.floor(Date.now() / 1000)
      setNow(timestamp)
      if (timestamp >= pendingExpiresAt) {
        setMaterials((value) => value?.kind === 'setup' ? undefined : value)
        setCurrentPassword('')
        setTotpCode('')
        window.clearInterval(timer)
      }
    }, 1000)
    return () => window.clearInterval(timer)
  }, [enabled, pendingExpiresAt])

  const showError = (error: unknown) => {
    const code = errorCode(error)
    setFormError(code === 'password_change_rejected' ? 'profile.errors.currentPassword'
      : code === 'two_factor_already_enabled' ? 'profile.twoFactor.alreadyEnabled'
        : code === 'two_factor_enrollment_invalid' ? 'profile.twoFactor.enrollmentInvalid'
          : 'profile.twoFactor.mutationError')
    if (code === 'two_factor_enrollment_invalid' || code === 'two_factor_already_enabled') void statusQuery.refetch()
  }

  const enroll = async (confirm: boolean) => {
    if (!currentPassword || working || (confirm && !/^\d{6}$/u.test(totpCode))) return
    setFormError(undefined)
    try {
      const response = await enableMutation.mutateAsync({
        current_password: currentPassword,
        ...(confirm ? { totp_code: totpCode } : {}),
      })
      if (!mounted.current) return
      setCurrentPassword('')
      setTotpCode('')
      setCopied(undefined)
      setCopyFailed(undefined)
      setNow(Math.floor(Date.now() / 1000))
      setMaterials(response.enabled
        ? { kind: 'backup', codes: [...response.backup_codes] }
        : response.pending_expires_at !== null
          ? { kind: 'setup', secret: response.secret, uri: response.otpauth_uri, expiresAt: response.pending_expires_at }
          : undefined)
    } catch (error) {
      if (mounted.current) showError(error)
    } finally {
      resetEnable()
    }
  }

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!currentPassword || working) return
    if (!enabled) {
      await enroll(pending)
      return
    }
    setFormError(undefined)
    try {
      await disableMutation.mutateAsync({ current_password: currentPassword })
      if (!mounted.current) return
      setMaterials(undefined)
      setCurrentPassword('')
      setTotpCode('')
    } catch (error) {
      if (mounted.current) showError(error)
    } finally {
      resetDisable()
    }
  }

  const copy = async (target: TwoFactorCopyTarget, value: string) => {
    const success = await copyText(value)
    if (!mounted.current) return
    setCopied(success ? target : undefined)
    setCopyFailed(success ? undefined : target)
  }
  const hintKey = enabled ? 'enabledHint' : expired ? 'expiredHint' : pending ? setupMaterials ? 'pendingHint' : 'resumeHint' : 'enableHint'
  const statusKey = enabled ? 'enabled' : expired ? 'expired' : pending ? 'pending' : 'disabled'

  return (
    <Card role="region" aria-label={t('profile.twoFactor.title')} shadow="none" className="overflow-hidden border border-[var(--hairline)]">
      <CardHeader className="gap-3 p-5">
        <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-default-100"><ShieldCheck className="size-4 text-muted-foreground" aria-hidden="true" /></div>
        <div className="min-w-0 flex-1"><h3 className="text-base font-semibold">{t('profile.twoFactor.title')}</h3><p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.twoFactor.subtitle')}</p></div>
        {statusQuery.isPending ? <Skeleton className="h-6 w-16 rounded-full" /> : status ? <Chip className={`shrink-0 ${enabled ? 'bg-primary/10 text-primary' : 'bg-default-100 text-default-500'}`} size="sm">{t(`profile.twoFactor.${statusKey}`)}</Chip> : null}
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
            <div className="rounded-xl bg-default-100 p-3 text-xs leading-5 text-muted-foreground">{t(`profile.twoFactor.${hintKey}`)}</div>
            {setupMaterials ? <EnrollmentMaterials materials={setupMaterials} copied={copied} copyFailed={copyFailed} onCopy={copy} onDismiss={() => setMaterials(undefined)} /> : null}
            {backupMaterials ? <EnrollmentMaterials materials={backupMaterials} copied={copied} copyFailed={copyFailed} onCopy={copy} onDismiss={() => setMaterials(undefined)} /> : null}
            {enabled ? (
              <div className="grid gap-1 text-xs text-muted-foreground">
                <p>{t('profile.twoFactor.boundAt', { value: formatPasskeyTimestamp(status?.bound_at ?? null) ?? t('profile.twoFactor.notRecorded') })}</p>
                <p>{t('profile.twoFactor.lastVerifiedAt', { value: formatPasskeyTimestamp(status?.last_verified_at ?? null) ?? t('profile.twoFactor.notRecorded') })}</p>
              </div>
            ) : status?.pending_created_at != null ? (
              <div className="grid gap-1 text-xs text-muted-foreground">
                <p>{t('profile.twoFactor.pendingAt', { value: formatPasskeyTimestamp(status.pending_created_at) })}</p>
                {pendingExpiresAt !== null ? <p>{t('profile.twoFactor.expiresAt', { value: formatPasskeyTimestamp(pendingExpiresAt) })}</p> : null}
              </div>
            ) : null}
            <form className="grid gap-3" onSubmit={(event) => void submit(event)} noValidate>
              {pending ? <div className="grid gap-2"><label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-two-factor-code">{t('profile.twoFactor.confirmCode')}</label><Input id="profile-two-factor-code" autoComplete="one-time-code" inputMode="numeric" maxLength={6} size="sm" value={totpCode} onChange={(event) => { setTotpCode(event.target.value.replace(/\D/gu, '').slice(0, 6)); setFormError(undefined) }} /></div> : null}
              <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
                <div className="grid gap-2">
                  <label className="text-xs font-medium leading-none text-foreground" htmlFor="profile-two-factor-password">{t('profile.twoFactor.currentPassword')}</label>
                  <Input autoComplete="current-password" id="profile-two-factor-password" isInvalid={formError === 'profile.errors.currentPassword'} size="sm" type="password" value={currentPassword} onChange={(event) => { setCurrentPassword(event.target.value); setFormError(undefined) }} />
                </div>
                <div className="flex flex-wrap gap-2">
                  {pending ? <Button type="button" size="sm" variant="bordered" isDisabled={!currentPassword || working} onClick={() => void enroll(false)}><RefreshCw className="size-4" aria-hidden="true" />{t('profile.twoFactor.regenerate')}</Button> : null}
                  <Button type="submit" size="sm" color="primary" isDisabled={!currentPassword || working || (pending && !/^\d{6}$/u.test(totpCode))} variant={enabled ? 'bordered' : 'solid'}>
                    {working ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
                    {t(working ? 'profile.twoFactor.working' : enabled ? 'profile.twoFactor.disable' : pending ? 'profile.twoFactor.confirm' : expired ? 'profile.twoFactor.regenerate' : 'profile.twoFactor.enable')}
                  </Button>
                </div>
              </div>
              {pending ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.twoFactor.confirmPasswordHint')}</p> : null}
              {enabled ? <p className="flex items-start gap-2 text-xs leading-5 text-muted-foreground"><ShieldOff className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />{t('profile.twoFactor.disableHint')}</p> : null}
              {formError ? <p role="alert" className="text-xs text-destructive">{t(formError)}</p> : null}
            </form>
          </div>
        )}
      </CardBody>
    </Card>
  )
}

function EnrollmentMaterials({ materials, copied, copyFailed, onCopy, onDismiss }: { materials: TwoFactorMaterials; copied?: TwoFactorCopyTarget; copyFailed?: TwoFactorCopyTarget; onCopy: (target: TwoFactorCopyTarget, value: string) => Promise<void>; onDismiss: () => void }) {
  const { t } = useTranslation()
  if (materials.kind === 'backup') {
    return (
      <div className="grid gap-4 rounded-xl border border-warning/25 bg-warning/6 p-4">
        <div>
          <p role="status" className="text-sm font-semibold">{t('profile.twoFactor.completed')}</p>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.twoFactor.backupHint')}</p>
        </div>
        <div className="grid gap-2">
          <div className="flex flex-wrap items-center justify-between gap-3"><span className="text-xs font-medium leading-none text-foreground">{t('profile.twoFactor.backupCodes')}</span><Button type="button" size="sm" variant="bordered" onClick={() => void onCopy('backup', materials.codes.join('\n'))}>{copied === 'backup' ? <Check className="size-4" aria-hidden="true" /> : <Copy className="size-4" aria-hidden="true" />}{t(copied === 'backup' ? 'profile.twoFactor.copied' : 'profile.twoFactor.copyBackup')}</Button></div>
          <div className="grid grid-cols-2 gap-2 rounded-lg border border-[var(--hairline)] bg-background/60 p-3 font-mono text-xs tabular-nums sm:grid-cols-5">{materials.codes.map((code) => <code key={code}>{code}</code>)}</div>
          {copyFailed === 'backup' ? <p role="alert" className="text-xs text-destructive">{t('profile.twoFactor.copyFailed')}</p> : null}
        </div>
        <div className="flex justify-end"><Button type="button" size="sm" variant="bordered" onClick={onDismiss}>{t('profile.twoFactor.savedBackup')}</Button></div>
      </div>
    )
  }
  return (
    <div className="grid gap-4 rounded-xl border border-[var(--hairline)] bg-surface-1/45 p-4">
      <div>
        <p className="text-sm font-semibold">{t('profile.twoFactor.materialsTitle')}</p>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.twoFactor.materialsHint')}</p>
      </div>
      <EnrollmentQr value={materials.uri} title={t('profile.twoFactor.qrCode')} />
      <CopyField label={t('profile.twoFactor.secret')} value={materials.secret} target="secret" copied={copied === 'secret'} copyFailed={copyFailed === 'secret'} onCopy={onCopy} />
      <CopyField label={t('profile.twoFactor.uri')} value={materials.uri} target="uri" copied={copied === 'uri'} copyFailed={copyFailed === 'uri'} onCopy={onCopy} />
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
      <CardHeader className="gap-3 p-5">
        <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-default-100"><Fingerprint className="size-4 text-muted-foreground" aria-hidden="true" /></div>
        <div className="min-w-0 flex-1"><h3 className="text-base font-semibold">{t('profile.passkeys.title')}</h3><p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.passkeys.subtitle')}</p></div>
        {passkeysQuery.data ? <Chip className="shrink-0 bg-default-100 text-foreground tabular-nums" size="sm">{t('profile.passkeys.count', { count: items.length })}</Chip> : null}
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
            <Button className="gap-1.5" type="submit" size="sm" color="primary" isDisabled={registrationPending}>
              {registrationPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Fingerprint className="size-3.5" aria-hidden="true" />}
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
                          <Button className="gap-1.5" type="button" size="sm" variant="bordered" onClick={() => beginRevoke(item.id)}><Trash2 className="size-3.5" aria-hidden="true" />{t('profile.passkeys.revoke')}</Button>
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
                        <div className="flex flex-wrap justify-end gap-2"><Button type="button" size="sm" variant="light" onClick={cancelRevoke}>{t('profile.passkeys.cancel')}</Button><Button className="gap-1.5" type="submit" size="sm" color="danger" variant="flat" isDisabled={!revokePassword || revokeMutation.isPending}>{revokeMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <Trash2 className="size-3.5" aria-hidden="true" />}{revokeMutation.isPending ? t('profile.passkeys.revoking') : t('profile.passkeys.confirmRevoke')}</Button></div>
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

  // 预警是系统侧的开关，不随个人偏好保存：按系统当前实际生效与否展示。
  const balanceAlertEnabled = profile.notifications.effective_balance_alert_threshold !== null
    && profile.notifications.effective_balance_alert_threshold !== undefined
  const subscriptionAlertEnabled = profile.notifications.subscription_remaining_percent > 0
  const thresholdInherited = thresholdMode === 'inherit'
  const thresholdLabel = thresholdInherited ? t('profile.notifications.thresholdInherit') : t('profile.notifications.thresholdCustom')
  const effectiveThreshold = thresholdMode === 'custom' && thresholdValid
    ? parsedThreshold
    : profile.notifications.effective_balance_alert_threshold

  return (
    <Card shadow="none" className="border border-[var(--hairline)]">
      <CardHeader className="gap-3 p-5">
        <div className="min-w-0 flex-1"><h3 className="text-base font-semibold">{t('profile.notifications.title')}</h3><p className="mt-1 text-sm leading-5 text-muted-foreground">{t('profile.notifications.subtitle')}</p></div>
      </CardHeader>
      <CardBody className="grid gap-5 p-5 lg:grid-cols-3">
        <section className="grid content-start gap-2">
          <p className="text-xs font-medium text-muted-foreground">{t('profile.notifications.groupEmail')}</p>
          <div className="divide-y divide-[var(--hairline)] rounded-xl border border-[var(--hairline)] px-4">
            <PreferenceRow id="profile-product-updates" title={t('profile.notifications.productTitle')} description={t('profile.notifications.productDescription')} checked={productUpdates} onCheckedChange={setProductUpdates} />
            <PreferenceRow id="profile-usage-alerts" title={t('profile.notifications.usageTitle')} description={t('profile.notifications.usageDescription')} checked={usageAlerts} onCheckedChange={setUsageAlerts} />
          </div>
        </section>

        <section className="grid content-start gap-2">
          <p className="text-xs font-medium text-muted-foreground">{t('profile.notifications.groupAlert')}</p>
          <div className="grid gap-4 rounded-xl border border-[var(--hairline)] p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <p className="text-sm font-medium">{t('profile.notifications.balanceAlertTitle')}</p>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.notifications.balanceAlertDescription')}</p>
              </div>
              {alertChip(balanceAlertEnabled, t(balanceAlertEnabled ? 'profile.notifications.balanceAlertActive' : 'profile.notifications.balanceAlertPaused'))}
            </div>
            <div className="grid items-end gap-3 sm:grid-cols-2">
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
                <Input
                  className="tabular-nums"
                  id="profile-balance-alert-threshold"
                  isDisabled={thresholdInherited}
                  isInvalid={!thresholdValid}
                  endContent={<span className="text-xs text-muted-foreground">{t('profile.notifications.quotaUnit')}</span>}
                  min={1}
                  size="sm"
                  step={1}
                  type="number"
                  value={thresholdInput}
                  onChange={(event) => setThresholdInput(event.target.value)}
                />
                {!thresholdValid ? <p className="text-xs text-destructive">{t('profile.notifications.thresholdInvalid')}</p> : null}
              </div>
            </div>
            <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-default-100 px-3 py-2 text-xs">
              <span className="text-muted-foreground">{t('profile.notifications.effectiveThreshold')}</span>
              <span className="flex items-center gap-2">
                <span className="text-muted-foreground">{thresholdLabel}</span>
                <span className="font-semibold tabular-nums">{effectiveThreshold}</span>
              </span>
            </div>
          </div>
        </section>

        <section className="grid content-start gap-2">
          <p className="text-xs font-medium text-muted-foreground">{t('profile.notifications.groupSystem')}</p>
          <div className="divide-y divide-[var(--hairline)] rounded-xl border border-[var(--hairline)] px-4">
            <InfoRow title={t('profile.notifications.subscriptionAlertTitle')} description={t('profile.notifications.subscriptionAlertDescription', { percent: profile.notifications.subscription_remaining_percent })} action={alertChip(subscriptionAlertEnabled, t(subscriptionAlertEnabled ? 'profile.notifications.subscriptionAlertActive' : 'profile.notifications.subscriptionAlertPaused'))} />
            <InfoRow title={t('profile.notifications.securityTitle')} description={t('profile.notifications.securityDescription')} action={<Chip className="shrink-0 bg-success/10 text-success" size="sm">{t('profile.notifications.alwaysOn')}</Chip>} />
          </div>
        </section>

        <div className="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] pt-4 lg:col-span-3"><div className="text-xs text-muted-foreground">{mutation.isSuccess ? <span className="flex items-center gap-1.5 text-success"><Check className="size-3.5" aria-hidden="true" />{t('profile.notifications.saved')}</span> : mutation.isError ? <span className="text-destructive">{t('profile.errors.notifications')}</span> : t('profile.notifications.footer')}</div><Button type="button" size="sm" color="primary" isDisabled={!changed || !thresholdValid || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}{t('profile.actions.saveNotifications')}</Button></div>
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
      <CardHeader className="gap-3 p-5">
        <div className="flex min-w-0 flex-1 gap-3">
          <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-default-100"><BellRing className="size-4 text-muted-foreground" aria-hidden="true" /></div>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-base font-semibold">{t('profile.notificationHistory.title')}</h3>
              {unreadCount !== undefined ? <Chip className={unreadCount > 0 ? 'bg-primary/10 text-primary tabular-nums' : 'bg-default-100 text-default-500 tabular-nums'} size="sm">{t('profile.notificationHistory.unreadCount', { count: unreadCount })}</Chip> : null}
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
    <div className="flex items-start gap-3 py-3"><div className="min-w-0 flex-1"><label className="text-sm font-medium" htmlFor={id}>{title}</label><p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p></div><Switch aria-label={title} id={id} isSelected={checked} size="sm" onValueChange={onCheckedChange} /></div>
  )
}

/** 只读信息行：系统侧状态，带描述与右侧状态徽标。 */
function InfoRow({ title, description, action }: { title: string; description: string; action: ReactNode }) {
  return <div className="flex flex-wrap items-start justify-between gap-3 py-3"><div className="min-w-0"><p className="text-sm font-medium">{title}</p><p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p></div>{action}</div>
}

/** 预警开关状态：启用用成功色，暂停用中性色——暂停不是告警，不该染成警告色。 */
function alertChip(enabled: boolean, label: string) {
  return <Chip className={`shrink-0 ${enabled ? 'bg-success/10 text-success' : 'bg-default-100 text-default-500'}`} size="sm">{label}</Chip>
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
    <div className="flex flex-col gap-6">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div><div className="mb-1 flex items-center gap-2 text-xs text-brand"><UserRound className="size-3.5" aria-hidden="true" />{roleLabel ?? t('profile.eyebrow')}</div><h2 className="text-lg font-semibold">{t('profile.title')}</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('profile.subtitle')}</p></div>
        <Button type="button" size="sm" variant="bordered" isDisabled={profileQuery.isFetching} onClick={() => void profileQuery.refetch()}><RefreshCw className={profileQuery.isFetching ? 'size-4 animate-spin' : 'size-4'} aria-hidden="true" />{t('profile.actions.refresh')}</Button>
      </header>
      {profileQuery.isPending ? <ProfileLoading /> : profileQuery.isError || !profile ? <ErrorPanel onRetry={() => void profileQuery.refetch()} /> : (
        <div className="flex flex-col gap-8">
          <ProfileCard profile={profile} roleLabel={roleLabel} onPasswordChanged={onPasswordChanged} />
          <ProfileSection title={t('profile.sections.security')}>
            <div className="grid gap-4 xl:grid-cols-2"><TwoFactorCard /><PasskeyCard onPasswordChanged={onPasswordChanged} /></div>
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
      <h3 className="text-xs font-medium text-muted-foreground">{title}</h3>
      {children}
    </section>
  )
}

