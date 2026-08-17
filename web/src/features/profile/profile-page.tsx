import { type FormEvent, useEffect, useMemo, useState } from 'react'
import {
  BellRing,
  CalendarClock,
  Check,
  CircleDollarSign,
  Copy,
  KeyRound,
  LoaderCircle,
  Mail,
  RefreshCw,
  ShieldCheck,
  ShieldOff,
  UserRound,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Skeleton } from '@/components/ui/skeleton'
import { Switch } from '@/components/ui/switch'
import { ApiError } from '@/lib/api'
import { copyText } from '@/lib/clipboard'
import type { UserProfileResponse, UserTwoFactorEnrollmentResponse } from '@/lib/api/generated/types.gen'
import { isValidProfilePassword, isValidProfileUsername } from './profile-form-model'
import {
  useChangeUserPassword,
  useDisableUserTwoFactor,
  useEnableUserTwoFactor,
  useUpdateUserNotificationPreferences,
  useUpdateUserProfile,
  useUserProfile,
  useUserTwoFactor,
} from './profile-api'

type ProfilePageProps = {
  onPasswordChanged: () => void
}

function errorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  const details = error.details as Record<string, unknown>
  return typeof details.code === 'string' ? details.code : undefined
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
      <Button type="button" size="sm" variant="secondary" className="mt-4" onClick={onRetry}>
        <RefreshCw aria-hidden="true" />{t('profile.actions.retry')}
      </Button>
    </div>
  )
}

function ProfileCard({ profile }: { profile: UserProfileResponse }) {
  const { t } = useTranslation()
  const updateMutation = useUpdateUserProfile()
  const [username, setUsername] = useState(profile.username)

  useEffect(() => setUsername(profile.username), [profile.username])

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
  return (
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex items-start justify-between gap-3">
          <div className="flex gap-3">
            <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-info/10 text-info"><UserRound className="size-4" aria-hidden="true" /></div>
            <div><CardTitle className="text-base">{t('profile.profile.title')}</CardTitle><CardDescription className="mt-1">{t('profile.profile.subtitle')}</CardDescription></div>
          </div>
          <Badge className={profile.role === 'admin' ? 'border-info/25 bg-info/10 text-info' : 'bg-surface-2 text-muted-foreground'}>
            {t(`auth.account.${profile.role}`)}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="p-5">
        <form className="grid gap-4" onSubmit={submit} noValidate>
          <div className="grid gap-2">
            <Label htmlFor="profile-username">{t('profile.profile.username')}</Label>
            <Input id="profile-username" value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" aria-invalid={username.length > 0 && !usernameValid} />
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.profile.usernameHint')}</p>
          </div>
          <div className="grid gap-2">
            <Label htmlFor="profile-email">{t('profile.profile.email')}</Label>
            <div className="relative">
              <Mail className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
              <Input id="profile-email" className="pl-9" value={profile.email ?? ''} placeholder={t('profile.profile.emailEmpty')} readOnly disabled />
            </div>
            <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.profile.emailHint')}</p>
          </div>
          {updateMutation.isSuccess ? <p role="status" className="flex items-center gap-1.5 text-xs text-success"><Check className="size-3.5" aria-hidden="true" />{t('profile.profile.saved')}</p> : null}
          {updateMutation.isError ? <p role="alert" className="text-xs text-destructive">{t(error === 'user_conflict' ? 'profile.errors.conflict' : 'profile.errors.save')}</p> : null}
          <div className="flex justify-end"><Button type="submit" size="sm" disabled={!changed || !usernameValid || updateMutation.isPending}>{updateMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('profile.actions.saveProfile')}</Button></div>
        </form>
      </CardContent>
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
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex gap-3">
          <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-warning/10 text-warning"><KeyRound className="size-4" aria-hidden="true" /></div>
          <div><CardTitle className="text-base">{t('profile.password.title')}</CardTitle><CardDescription className="mt-1">{t('profile.password.subtitle')}</CardDescription></div>
        </div>
      </CardHeader>
      <CardContent className="p-5">
        <form className="grid gap-4" onSubmit={submit} noValidate>
          <div className="grid gap-2"><Label htmlFor="profile-current-password">{t('profile.password.current')}</Label><Input id="profile-current-password" type="password" autoComplete="current-password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} /></div>
          <div className="grid gap-2"><Label htmlFor="profile-new-password">{t('profile.password.next')}</Label><Input id="profile-new-password" type="password" autoComplete="new-password" value={newPassword} onChange={(event) => setNewPassword(event.target.value)} aria-invalid={newPassword.length > 0 && !isValidProfilePassword(newPassword)} /><p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('profile.password.hint')}</p></div>
          <div className="grid gap-2"><Label htmlFor="profile-confirm-password">{t('profile.password.confirm')}</Label><Input id="profile-confirm-password" type="password" autoComplete="new-password" value={confirmation} onChange={(event) => setConfirmation(event.target.value)} aria-invalid={confirmation.length > 0 && confirmation !== newPassword} /></div>
          {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t(code === 'password_change_rejected' ? 'profile.errors.currentPassword' : 'profile.errors.password')}</p> : null}
          <div className="flex items-start gap-2 rounded-xl border border-warning/20 bg-warning/6 p-3 text-[0.6875rem] leading-4 text-muted-foreground"><ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-warning" aria-hidden="true" /><span>{t('profile.password.revokesSessions')}</span></div>
          <div className="flex justify-end"><Button type="submit" size="sm" disabled={!valid || mutation.isPending}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('profile.actions.changePassword')}</Button></div>
        </form>
      </CardContent>
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
    <Card className="overflow-hidden xl:col-span-2">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex items-start justify-between gap-3">
          <div className="flex gap-3">
            <div className="grid size-9 shrink-0 place-items-center rounded-xl bg-success/10 text-success"><ShieldCheck className="size-4" aria-hidden="true" /></div>
            <div><CardTitle className="text-base">{t('profile.twoFactor.title')}</CardTitle><CardDescription className="mt-1">{t('profile.twoFactor.subtitle')}</CardDescription></div>
          </div>
          {statusQuery.data ? <Badge className={enabled ? 'bg-success/10 text-success' : 'bg-surface-2 text-muted-foreground'}>{t(enabled ? 'profile.twoFactor.enabled' : 'profile.twoFactor.disabled')}</Badge> : null}
        </div>
      </CardHeader>
      <CardContent className="p-5">
        {statusQuery.isPending ? (
          <div className="grid gap-3" role="status"><Skeleton className="h-4 w-48" /><Skeleton className="h-11 w-full" /></div>
        ) : statusQuery.isError ? (
          <div role="alert" className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-destructive/20 bg-destructive/8 p-3 text-xs text-destructive">
            <span>{t('profile.twoFactor.loadError')}</span>
            <Button type="button" size="sm" variant="secondary" onClick={() => void statusQuery.refetch()}><RefreshCw aria-hidden="true" />{t('profile.actions.retry')}</Button>
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
                <Label htmlFor="profile-two-factor-password">{t('profile.twoFactor.currentPassword')}</Label>
                <Input id="profile-two-factor-password" type="password" autoComplete="current-password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} aria-invalid={mutation.isError ? true : undefined} />
              </div>
              <Button type="submit" size="sm" variant={enabled ? 'secondary' : 'default'} disabled={!currentPassword || mutation.isPending}>
                {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
                {mutation.isPending ? t('profile.twoFactor.working') : t(enabled ? 'profile.twoFactor.disable' : 'profile.twoFactor.enable')}
              </Button>
            </form>
            {mutation.isError ? <p role="alert" className="text-xs text-destructive">{t(errorCode(mutation.error) === 'password_change_rejected' ? 'profile.errors.currentPassword' : errorCode(mutation.error) === 'two_factor_already_enabled' ? 'profile.twoFactor.alreadyEnabled' : 'profile.twoFactor.mutationError')}</p> : null}
          </div>
        )}
      </CardContent>
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
        <div className="flex items-center justify-between gap-3"><Label>{t('profile.twoFactor.backupCodes')}</Label><Button type="button" size="sm" variant="secondary" onClick={() => void onCopy('backup', enrollment.backup_codes.join('\n'))}>{copied === 'backup' ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}{t(copied === 'backup' ? 'profile.twoFactor.copied' : 'profile.twoFactor.copyBackup')}</Button></div>
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
      <Label>{label}</Label>
      <div className="flex items-center gap-2">
        <code className="min-w-0 flex-1 break-all rounded-lg border border-[var(--hairline)] bg-background/60 px-3 py-2 text-xs leading-5">{value}</code>
        <Button type="button" size="icon-sm" variant="secondary" title={t(copied ? 'profile.twoFactor.copied' : 'profile.twoFactor.copy')} aria-label={t(copied ? 'profile.twoFactor.copied' : 'profile.twoFactor.copy')} onClick={() => void onCopy(target, value)}>{copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}</Button>
      </div>
      {copyFailed ? <p role="alert" className="text-xs text-destructive">{t('profile.twoFactor.copyFailed')}</p> : null}
    </div>
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
    <Card className="overflow-hidden">
      <CardHeader className="border-b border-[var(--hairline)] bg-surface-1/45">
        <div className="flex gap-3"><div className="grid size-9 shrink-0 place-items-center rounded-xl bg-brand/10 text-brand"><BellRing className="size-4" aria-hidden="true" /></div><div><CardTitle className="text-base">{t('profile.notifications.title')}</CardTitle><CardDescription className="mt-1">{t('profile.notifications.subtitle')}</CardDescription></div></div>
      </CardHeader>
      <CardContent className="p-5">
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
              <Badge className={profile.notifications.balance_alert_enabled ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'}>
                {t(profile.notifications.balance_alert_enabled
                  ? 'profile.notifications.balanceAlertActive'
                  : 'profile.notifications.balanceAlertPaused')}
              </Badge>
            </div>
            <div className="mt-4 grid gap-3 sm:grid-cols-2">
              <div className="grid gap-2">
                <Label htmlFor="profile-balance-alert-mode">{t('profile.notifications.thresholdMode')}</Label>
                <Select
                  id="profile-balance-alert-mode"
                  value={thresholdMode}
                  onChange={(event) => setThresholdMode(event.target.value as 'inherit' | 'custom')}
                >
                  <option value="inherit">{t('profile.notifications.thresholdInherit')}</option>
                  <option value="custom">{t('profile.notifications.thresholdCustom')}</option>
                </Select>
              </div>
              <div className="grid gap-2">
                <Label htmlFor="profile-balance-alert-threshold">{t('profile.notifications.thresholdValue')}</Label>
                <div className="relative">
                  <Input
                    id="profile-balance-alert-threshold"
                    type="number"
                    min={1}
                    step={1}
                    className="pr-16 tabular-nums"
                    value={thresholdInput}
                    disabled={thresholdMode === 'inherit'}
                    aria-invalid={!thresholdValid}
                    onChange={(event) => setThresholdInput(event.target.value)}
                  />
                  <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-muted-foreground">
                    {t('profile.notifications.quotaUnit')}
                  </span>
                </div>
                {!thresholdValid ? <p className="text-xs text-destructive">{t('profile.notifications.thresholdInvalid')}</p> : null}
              </div>
            </div>
            <div className="mt-3 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <span>{t('profile.notifications.effectiveThreshold')}</span>
              <Badge className="bg-info/10 font-mono text-info">
                {thresholdMode === 'custom' && thresholdValid
                  ? parsedThreshold
                  : profile.notifications.effective_balance_alert_threshold}
              </Badge>
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
            <Badge className={profile.notifications.subscription_alert_enabled ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning'}>
              {t(profile.notifications.subscription_alert_enabled
                ? 'profile.notifications.subscriptionAlertActive'
                : 'profile.notifications.subscriptionAlertPaused')}
            </Badge>
          </div>
          <div className="flex items-start gap-3 py-4"><ShieldCheck className="mt-0.5 size-4 shrink-0 text-success" aria-hidden="true" /><div className="min-w-0"><p className="text-sm font-medium">{t('profile.notifications.securityTitle')}</p><p className="mt-1 text-xs leading-5 text-muted-foreground">{t('profile.notifications.securityDescription')}</p></div><Badge className="ml-auto shrink-0 bg-success/10 text-success">{t('profile.notifications.alwaysOn')}</Badge></div>
        </div>
        <Separator className="my-4" />
        <div className="flex flex-wrap items-center justify-between gap-3"><div className="text-xs text-muted-foreground">{mutation.isSuccess ? <span className="flex items-center gap-1.5 text-success"><Check className="size-3.5" aria-hidden="true" />{t('profile.notifications.saved')}</span> : mutation.isError ? <span className="text-destructive">{t('profile.errors.notifications')}</span> : t('profile.notifications.footer')}</div><Button type="button" size="sm" disabled={!changed || !thresholdValid || mutation.isPending} onClick={() => void submit()}>{mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t('profile.actions.saveNotifications')}</Button></div>
      </CardContent>
    </Card>
  )
}

function PreferenceRow({ id, title, description, checked, onCheckedChange }: { id: string; title: string; description: string; checked: boolean; onCheckedChange: (checked: boolean) => void }) {
  return (
    <div className="flex items-start gap-3 py-4 first:pt-0 last:pb-0"><div className="min-w-0 flex-1"><Label htmlFor={id} className="text-sm font-medium">{title}</Label><p className="mt-1 text-xs leading-5 text-muted-foreground">{description}</p></div><Switch id={id} checked={checked} onCheckedChange={onCheckedChange} aria-label={title} /></div>
  )
}

/** 组合当前用户的资料、密码和通知偏好设置，并保持敏感输入只存在于内存。 */
export function ProfilePage({ onPasswordChanged }: ProfilePageProps) {
  const { t } = useTranslation()
  const profileQuery = useUserProfile()
  const profile = profileQuery.data
  const roleLabel = useMemo(() => profile ? t(`auth.account.${profile.role}`) : undefined, [profile, t])

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-col gap-3 sm:flex-row sm:items-end sm:justify-between">
        <div><div className="mb-1 flex items-center gap-2 text-[0.6875rem] uppercase tracking-[0.18em] text-brand"><UserRound className="size-3.5" aria-hidden="true" />{roleLabel ?? t('profile.eyebrow')}</div><h2 className="text-lg font-semibold">{t('profile.title')}</h2><p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{t('profile.subtitle')}</p></div>
        <Button type="button" size="sm" variant="secondary" disabled={profileQuery.isFetching} onClick={() => void profileQuery.refetch()}><RefreshCw className={profileQuery.isFetching ? 'animate-spin' : undefined} aria-hidden="true" />{t('profile.actions.refresh')}</Button>
      </header>
      {profileQuery.isPending ? <ProfileLoading /> : profileQuery.isError || !profile ? <ErrorPanel onRetry={() => void profileQuery.refetch()} /> : <div className="grid gap-4 xl:grid-cols-2"><ProfileCard profile={profile} /><PasswordCard onPasswordChanged={onPasswordChanged} /><TwoFactorCard /><div className="xl:col-span-2"><NotificationsCard profile={profile} /></div></div>}
    </div>
  )
}
