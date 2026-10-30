import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Fingerprint, LoaderCircle, RefreshCw, ShieldCheck, ShieldOff, Trash2 } from 'lucide-react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { useAdminGroupCatalog } from '@/features/groups/group-api'
import type { AdminGroup, AdminUser } from '@/lib/api/generated/types.gen'
import { isUserConflict, useAdminUserSecurity, useCreateAdminUser, useResetAdminUserTwoFactor, useRevokeAdminUserPasskey, useUpdateAdminUser } from './user-api'
import { UserFormField } from './user-form-field'
import {
  buildUserFormSchema,
  defaultUserValues,
  toUserCreateRequest,
  toUserUpdateRequest,
  type UserEditorMode,
  type UserFormValues,
} from './user-form-model'

type UserFormProps = {
  mode: UserEditorMode
  user?: AdminUser
  onCancel: () => void
  onSaved: (user: AdminUser) => void
}

const EMPTY_GROUPS: AdminGroup[] = []

export function UserForm({ mode, user, onCancel, onSaved }: UserFormProps) {
  const { t } = useTranslation()
  const groupsQuery = useAdminGroupCatalog()
  const createMutation = useCreateAdminUser()
  const updateMutation = useUpdateAdminUser()
  const schema = buildUserFormSchema(mode, {
    invalidUsername: t('users.validation.username'),
    invalidEmail: t('users.validation.email'),
    invalidPassword: t('users.validation.password'),
    invalidGroup: t('users.validation.group'),
    invalidNumber: t('users.validation.number'),
  })
  const form = useForm<UserFormValues>({
    defaultValues: defaultUserValues(user),
    resolver: zodResolver(schema),
  })
  const groups = groupsQuery.data ?? EMPTY_GROUPS
  const selectedGroupId = form.watch('defaultGroupId')
  const selectedGroupExists = groups.some((group) => String(group.id) === selectedGroupId)

  useEffect(() => {
    form.reset(defaultUserValues(user))
  }, [form, mode, user])

  useEffect(() => {
    if (mode === 'create' && groups[0] && !form.getValues('defaultGroupId')) {
      form.setValue('defaultGroupId', String(groups[0].id))
    }
  }, [form, groups, mode])

  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.error ?? updateMutation.error
  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const saved = mode === 'create'
        ? await createMutation.mutateAsync(toUserCreateRequest(values))
        : user
          ? await updateMutation.mutateAsync({ id: user.id, body: toUserUpdateRequest(values) })
          : undefined
      if (saved) onSaved(saved)
    } catch {
      // Mutation 状态负责保留结构化错误，表单内容保持不变以便修正。
    }
  })

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={onSubmit} noValidate>
      <div className="flex-1 space-y-6 overflow-y-auto px-4 py-5">
        <section className="grid gap-3" aria-labelledby="user-account-fields">
          <h3 id="user-account-fields" className="text-xs font-semibold">{t('users.form.account')}</h3>
          <UserFormField id="user-username" label={t('users.fields.username')} error={form.formState.errors.username?.message}>
            <Input id="user-username" autoComplete="off" aria-invalid={!!form.formState.errors.username} {...form.register('username')} />
          </UserFormField>
          <UserFormField id="user-email" label={t('users.fields.email')} hint={t('users.form.emailHint')} error={form.formState.errors.email?.message}>
            <Input id="user-email" type="email" autoComplete="off" aria-invalid={!!form.formState.errors.email} {...form.register('email')} />
          </UserFormField>
          <UserFormField id="user-password" label={t('users.fields.password')} hint={t(mode === 'create' ? 'users.form.createPasswordHint' : 'users.form.updatePasswordHint')} error={form.formState.errors.password?.message}>
            <Input id="user-password" type="password" autoComplete="new-password" aria-invalid={!!form.formState.errors.password} {...form.register('password')} />
          </UserFormField>
        </section>

        <section className="grid gap-3" aria-labelledby="user-access-fields">
          <h3 id="user-access-fields" className="text-xs font-semibold">{t('users.form.access')}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <UserFormField id="user-role" label={t('users.fields.role')}>
              <Select id="user-role" {...form.register('role')}>
                <option value="user">{t('users.role.user')}</option>
                <option value="admin">{t('users.role.admin')}</option>
              </Select>
            </UserFormField>
            <UserFormField id="user-status" label={t('users.fields.status')}>
              <Select id="user-status" {...form.register('status')}>
                <option value="enabled">{t('users.status.enabled')}</option>
                <option value="disabled">{t('users.status.disabled')}</option>
              </Select>
            </UserFormField>
          </div>
          <UserFormField id="user-group" label={t('users.fields.defaultGroup')} hint={groupsQuery.isError ? undefined : t('users.form.groupHint')} error={form.formState.errors.defaultGroupId?.message}>
            <Select id="user-group" disabled={groupsQuery.isPending || groupsQuery.isError || groups.length === 0} aria-invalid={!!form.formState.errors.defaultGroupId} {...form.register('defaultGroupId')}>
              <option value="">{t(groupsQuery.isPending ? 'users.form.groupsLoading' : groups.length === 0 ? 'users.form.groupsEmpty' : 'users.form.groupPlaceholder')}</option>
              {selectedGroupId && !selectedGroupExists ? <option value={selectedGroupId}>#{selectedGroupId}</option> : null}
              {groups.map((group) => <option key={group.id} value={group.id}>{group.display_name} ({group.name})</option>)}
            </Select>
            {groupsQuery.isError ? (
              <div className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
                <span>{t('users.form.groupsFailed')}</span>
                <Button type="button" size="sm" variant="ghost" onClick={() => void groupsQuery.refetch()}><RefreshCw aria-hidden="true" />{t('users.actions.retry')}</Button>
              </div>
            ) : null}
          </UserFormField>
        </section>

        <section className="grid gap-3" aria-labelledby="user-limit-fields">
          <h3 id="user-limit-fields" className="text-xs font-semibold">{t('users.form.limits')}</h3>
          {mode === 'create' ? (
            <UserFormField id="user-quota" label={t('users.fields.initialQuota')} hint={t('users.form.quotaHint')} error={form.formState.errors.quota?.message}>
              <Input id="user-quota" type="number" min={0} step={1} inputMode="numeric" aria-invalid={!!form.formState.errors.quota} {...form.register('quota', { valueAsNumber: true })} />
            </UserFormField>
          ) : null}
          <div className="grid gap-3 sm:grid-cols-2">
            <UserFormField id="user-rpm" label={t('users.fields.rpmLimit')} hint={t('users.form.optionalLimitHint')} error={form.formState.errors.rpmLimit?.message}>
              <Input id="user-rpm" type="number" min={0} max={2_147_483_647} step={1} inputMode="numeric" aria-invalid={!!form.formState.errors.rpmLimit} {...form.register('rpmLimit')} />
            </UserFormField>
            <UserFormField id="user-concurrency" label={t('users.fields.concurrency')} hint={t('users.form.optionalLimitHint')} error={form.formState.errors.concurrency?.message}>
              <Input id="user-concurrency" type="number" min={0} max={2_147_483_647} step={1} inputMode="numeric" aria-invalid={!!form.formState.errors.concurrency} {...form.register('concurrency')} />
            </UserFormField>
          </div>
        </section>

        {mode === 'update' && user ? <AdminSecuritySection userId={user.id} /> : null}

        {submitError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t(isUserConflict(submitError) ? 'users.form.conflict' : 'users.form.submitError')}</p> : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="secondary" onClick={onCancel}>{t('users.actions.cancel')}</Button>
        <Button type="submit" disabled={pending || groupsQuery.isPending || groupsQuery.isError || groups.length === 0}>
          {pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(mode === 'create' ? 'users.actions.create' : 'users.actions.save')}
        </Button>
      </div>
    </form>
  )
}

function AdminSecuritySection({ userId }: { userId: number }) {
  const { t, i18n } = useTranslation()
  const securityQuery = useAdminUserSecurity(userId)
  const resetTwoFactorMutation = useResetAdminUserTwoFactor(userId)
  const revokePasskeyMutation = useRevokeAdminUserPasskey(userId)
  const formatTime = (value: number | null | undefined) => value == null
    ? t('users.security.notRecorded')
    : new Intl.DateTimeFormat(i18n.language, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value * 1000))

  const resetTwoFactor = async () => {
    if (!window.confirm(t('users.security.resetTwoFactorConfirm'))) return
    try {
      await resetTwoFactorMutation.mutateAsync()
    } catch {
      // 错误状态在区域内展示，保留当前编辑内容。
    }
  }

  const revokePasskey = async (passkeyId: number) => {
    if (!window.confirm(t('users.security.revokePasskeyConfirm'))) return
    try {
      await revokePasskeyMutation.mutateAsync(passkeyId)
    } catch {
      // 错误状态在区域内展示，保留当前编辑内容。
    }
  }

  return (
    <section className="grid gap-3" aria-labelledby="user-security-fields">
      <h3 id="user-security-fields" className="text-xs font-semibold">{t('users.form.security')}</h3>
      {securityQuery.isPending ? <p role="status" className="text-xs text-muted-foreground">{t('users.security.loading')}</p> : securityQuery.isError ? (
        <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
          <span>{t('users.security.loadError')}</span>
          <Button type="button" size="sm" variant="ghost" onClick={() => void securityQuery.refetch()}><RefreshCw aria-hidden="true" />{t('users.actions.retry')}</Button>
        </div>
      ) : securityQuery.data ? (
        <div className="grid gap-4 rounded-lg border border-[var(--hairline)] p-3">
          <div className="grid gap-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2 text-sm font-medium"><ShieldCheck className="size-4 text-muted-foreground" aria-hidden="true" />{t('users.security.twoFactor')}</div>
              <span className={`rounded-full px-2 py-0.5 text-[0.6875rem] font-medium ${securityQuery.data.two_factor.enabled ? 'bg-success/10 text-success' : 'bg-muted text-muted-foreground'}`}>{t(`users.security.states.${securityQuery.data.two_factor.state}`)}</span>
            </div>
            {securityQuery.data.two_factor.enabled ? <div className="grid gap-1 text-xs text-muted-foreground"><p>{t('users.security.boundAt', { value: formatTime(securityQuery.data.two_factor.bound_at) })}</p><p>{t('users.security.lastVerifiedAt', { value: formatTime(securityQuery.data.two_factor.last_verified_at) })}</p></div> : securityQuery.data.two_factor.state === 'pending' ? <div className="grid gap-1 text-xs text-muted-foreground"><p>{t('users.security.pendingAt', { value: formatTime(securityQuery.data.two_factor.pending_created_at) })}</p><p>{t('users.security.expiresAt', { value: formatTime(securityQuery.data.two_factor.pending_expires_at) })}</p></div> : null}
            {securityQuery.data.two_factor.state !== 'disabled' ? <Button type="button" size="sm" variant="secondary" className="w-fit" disabled={resetTwoFactorMutation.isPending} onClick={() => void resetTwoFactor()}>{resetTwoFactorMutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : <ShieldOff aria-hidden="true" />}{t('users.security.resetTwoFactor')}</Button> : null}
          </div>
          <div className="grid gap-2 border-t border-[var(--hairline)] pt-3">
            <div className="flex items-center gap-2 text-sm font-medium"><Fingerprint className="size-4 text-muted-foreground" aria-hidden="true" />{t('users.security.passkeys')}<span className="text-xs font-normal text-muted-foreground">{t('users.security.passkeyCount', { count: securityQuery.data.passkeys.length })}</span></div>
            {securityQuery.data.passkeys.length === 0 ? <p className="text-xs text-muted-foreground">{t('users.security.noPasskeys')}</p> : <div className="grid gap-2">{securityQuery.data.passkeys.map((passkey) => <div key={passkey.id} className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-muted/40 px-2.5 py-2"><div className="min-w-0"><p className="truncate text-xs font-medium">{passkey.display_name}</p><p className="text-[0.6875rem] text-muted-foreground">{t('users.security.passkeyCreatedAt', { value: formatTime(passkey.created_at) })}{passkey.last_used_at != null ? ` · ${t('users.security.passkeyLastUsedAt', { value: formatTime(passkey.last_used_at) })}` : ''}</p></div>{passkey.revoked_at == null ? <Button type="button" size="sm" variant="ghost" className="text-destructive" disabled={revokePasskeyMutation.isPending} onClick={() => void revokePasskey(passkey.id)}><Trash2 aria-hidden="true" />{t('users.security.revokePasskey')}</Button> : <span className="text-[0.6875rem] text-muted-foreground">{t('users.security.revoked')}</span>}</div>)}</div>}
          </div>
          {resetTwoFactorMutation.isError || revokePasskeyMutation.isError ? <p role="alert" className="text-xs text-destructive">{t('users.security.actionError')}</p> : null}
        </div>
      ) : null}
    </section>
  )
}
