import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Input, Select, SelectItem } from '@heroui/react'
import { Fingerprint, LoaderCircle, RefreshCw, ShieldCheck, ShieldOff, Trash2 } from 'lucide-react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

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

/** HeroUI Select 不接受空字符串 key，用哨兵 key 表达"未选择默认分组"。 */
const NO_GROUP_KEY = '__none__'

const ROLE_ITEMS = [
  { key: 'user', labelKey: 'users.role.user' },
  { key: 'admin', labelKey: 'users.role.admin' },
] as const

const STATUS_ITEMS = [
  { key: 'enabled', labelKey: 'users.status.enabled' },
  { key: 'disabled', labelKey: 'users.status.disabled' },
] as const

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
  // HeroUI Select 的动态选项必须走 items + 渲染函数（数组子节点不被类型接受）。
  const groupItems = [
    {
      key: NO_GROUP_KEY,
      label: t(groupsQuery.isPending ? 'users.form.groupsLoading' : groups.length === 0 ? 'users.form.groupsEmpty' : 'users.form.groupPlaceholder'),
    },
    ...(selectedGroupId && !selectedGroupExists ? [{ key: selectedGroupId, label: `#${selectedGroupId}` }] : []),
    ...groups.map((group) => ({ key: String(group.id), label: `${group.display_name} (${group.name})` })),
  ]
  const roleItems = ROLE_ITEMS.map((item) => ({ key: item.key, label: t(item.labelKey) }))
  const statusItems = STATUS_ITEMS.map((item) => ({ key: item.key, label: t(item.labelKey) }))

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
      <div className="flex-1 space-y-6 overflow-y-auto px-4 pt-4 pb-5">
        <section className="grid gap-3" aria-labelledby="user-account-fields">
          <h3 id="user-account-fields" className="text-xs font-semibold">{t('users.form.account')}</h3>
          <UserFormField id="user-username" label={t('users.fields.username')} error={form.formState.errors.username?.message}>
            <Input autoComplete="off" id="user-username" isInvalid={!!form.formState.errors.username} size="sm" {...form.register('username')} />
          </UserFormField>
          <UserFormField id="user-email" label={t('users.fields.email')} hint={t('users.form.emailHint')} error={form.formState.errors.email?.message}>
            <Input autoComplete="off" id="user-email" isInvalid={!!form.formState.errors.email} size="sm" type="email" {...form.register('email')} />
          </UserFormField>
          <UserFormField id="user-password" label={t('users.fields.password')} hint={t(mode === 'create' ? 'users.form.createPasswordHint' : 'users.form.updatePasswordHint')} error={form.formState.errors.password?.message}>
            <Input autoComplete="new-password" id="user-password" isInvalid={!!form.formState.errors.password} size="sm" type="password" {...form.register('password')} />
          </UserFormField>
        </section>

        <section className="grid gap-3" aria-labelledby="user-access-fields">
          <h3 id="user-access-fields" className="text-xs font-semibold">{t('users.form.access')}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <UserFormField id="user-role" label={t('users.fields.role')}>
              <Controller
                control={form.control}
                name="role"
                render={({ field }) => (
                  <Select
                    aria-label={t('users.fields.role')}
                    id="user-role"
                    items={roleItems}
                    selectedKeys={[field.value]}
                    size="sm"
                    onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? 'user'))}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                )}
              />
            </UserFormField>
            <UserFormField id="user-status" label={t('users.fields.status')}>
              <Controller
                control={form.control}
                name="status"
                render={({ field }) => (
                  <Select
                    aria-label={t('users.fields.status')}
                    id="user-status"
                    items={statusItems}
                    selectedKeys={[field.value]}
                    size="sm"
                    onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? 'enabled'))}
                  >
                    {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                  </Select>
                )}
              />
            </UserFormField>
          </div>
          <UserFormField id="user-group" label={t('users.fields.defaultGroup')} hint={groupsQuery.isError ? undefined : t('users.form.groupHint')} error={form.formState.errors.defaultGroupId?.message}>
            <Controller
              control={form.control}
              name="defaultGroupId"
              render={({ field }) => (
                <Select
                  aria-label={t('users.fields.defaultGroup')}
                  id="user-group"
                  isDisabled={groupsQuery.isPending || groupsQuery.isError || groups.length === 0}
                  isInvalid={!!form.formState.errors.defaultGroupId}
                  items={groupItems}
                  selectedKeys={[field.value === '' ? NO_GROUP_KEY : field.value]}
                  size="sm"
                  onSelectionChange={(keys) => {
                    const next = String(Array.from(keys)[0] ?? NO_GROUP_KEY)
                    field.onChange(next === NO_GROUP_KEY ? '' : next)
                  }}
                >
                  {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
                </Select>
              )}
            />
            {groupsQuery.isError ? (
              <div className="flex items-center justify-between gap-3 rounded-lg bg-destructive/8 px-3 py-2 text-xs text-destructive">
                <span>{t('users.form.groupsFailed')}</span>
                <Button type="button" size="sm" variant="light" onClick={() => void groupsQuery.refetch()}><RefreshCw className="size-3.5" aria-hidden="true" />{t('users.actions.retry')}</Button>
              </div>
            ) : null}
          </UserFormField>
        </section>

        <section className="grid gap-3" aria-labelledby="user-limit-fields">
          <h3 id="user-limit-fields" className="text-xs font-semibold">{t('users.form.limits')}</h3>
          {mode === 'create' ? (
            <UserFormField id="user-quota" label={t('users.fields.initialQuota')} hint={t('users.form.quotaHint')} error={form.formState.errors.quota?.message}>
              <Input id="user-quota" inputMode="numeric" isInvalid={!!form.formState.errors.quota} min={0} size="sm" step={1} type="number" {...form.register('quota', { valueAsNumber: true })} />
            </UserFormField>
          ) : null}
          <div className="grid gap-3 sm:grid-cols-2">
            <UserFormField id="user-rpm" label={t('users.fields.rpmLimit')} hint={t('users.form.optionalLimitHint')} error={form.formState.errors.rpmLimit?.message}>
              <Input id="user-rpm" inputMode="numeric" isInvalid={!!form.formState.errors.rpmLimit} max={2_147_483_647} min={0} size="sm" step={1} type="number" {...form.register('rpmLimit')} />
            </UserFormField>
            <UserFormField id="user-concurrency" label={t('users.fields.concurrency')} hint={t('users.form.optionalLimitHint')} error={form.formState.errors.concurrency?.message}>
              <Input id="user-concurrency" inputMode="numeric" isInvalid={!!form.formState.errors.concurrency} max={2_147_483_647} min={0} size="sm" step={1} type="number" {...form.register('concurrency')} />
            </UserFormField>
          </div>
        </section>

        {mode === 'update' && user ? <AdminSecuritySection userId={user.id} /> : null}

        {submitError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t(isUserConflict(submitError) ? 'users.form.conflict' : 'users.form.submitError')}</p> : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="bordered" onClick={onCancel}>{t('users.actions.cancel')}</Button>
        <Button color="primary" isDisabled={pending || groupsQuery.isPending || groupsQuery.isError || groups.length === 0} type="submit">
          {pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
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
        <div role="alert" className="flex items-center justify-between gap-3 rounded-lg bg-danger/8 px-3 py-2 text-xs text-danger">
          <span>{t('users.security.loadError')}</span>
          <Button type="button" size="sm" variant="light" onPress={() => void securityQuery.refetch()}><RefreshCw className="size-3.5" aria-hidden="true" />{t('users.actions.retry')}</Button>
        </div>
      ) : securityQuery.data ? (
        <div className="grid gap-4 rounded-lg border border-[var(--hairline)] p-3">
          <div className="grid gap-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center gap-2 text-sm font-medium"><ShieldCheck className="size-4 text-default-500" aria-hidden="true" />{t('users.security.twoFactor')}</div>
              <span className={`rounded-full px-2 py-0.5 text-[0.6875rem] font-medium ${securityQuery.data.two_factor.enabled ? 'bg-success/10 text-success' : 'bg-default-100 text-default-500'}`}>{t(`users.security.states.${securityQuery.data.two_factor.state}`)}</span>
            </div>
            {securityQuery.data.two_factor.enabled ? <div className="grid gap-1 text-xs text-default-500"><p>{t('users.security.boundAt', { value: formatTime(securityQuery.data.two_factor.bound_at) })}</p><p>{t('users.security.lastVerifiedAt', { value: formatTime(securityQuery.data.two_factor.last_verified_at) })}</p></div> : securityQuery.data.two_factor.state === 'pending' ? <div className="grid gap-1 text-xs text-default-500"><p>{t('users.security.pendingAt', { value: formatTime(securityQuery.data.two_factor.pending_created_at) })}</p><p>{t('users.security.expiresAt', { value: formatTime(securityQuery.data.two_factor.pending_expires_at) })}</p></div> : null}
            {securityQuery.data.two_factor.state !== 'disabled' ? <Button type="button" size="sm" variant="flat" className="w-fit" isDisabled={resetTwoFactorMutation.isPending} onPress={() => void resetTwoFactor()}>{resetTwoFactorMutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : <ShieldOff className="size-4" aria-hidden="true" />}{t('users.security.resetTwoFactor')}</Button> : null}
          </div>
          <div className="grid gap-2 border-t border-[var(--hairline)] pt-3">
            <div className="flex items-center gap-2 text-sm font-medium"><Fingerprint className="size-4 text-default-500" aria-hidden="true" />{t('users.security.passkeys')}<span className="text-xs font-normal text-default-500">{t('users.security.passkeyCount', { count: securityQuery.data.passkeys.length })}</span></div>
            {securityQuery.data.passkeys.length === 0 ? <p className="text-xs text-default-500">{t('users.security.noPasskeys')}</p> : <div className="grid gap-2">{securityQuery.data.passkeys.map((passkey) => <div key={passkey.id} className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-default-100/60 px-2.5 py-2"><div className="min-w-0"><p className="truncate text-xs font-medium">{passkey.display_name}</p><p className="text-[0.6875rem] text-default-500">{t('users.security.passkeyCreatedAt', { value: formatTime(passkey.created_at) })}{passkey.last_used_at != null ? ` · ${t('users.security.passkeyLastUsedAt', { value: formatTime(passkey.last_used_at) })}` : ''}</p></div>{passkey.revoked_at == null ? <Button type="button" size="sm" variant="light" className="text-danger" isDisabled={revokePasskeyMutation.isPending} onPress={() => void revokePasskey(passkey.id)}><Trash2 className="size-4" aria-hidden="true" />{t('users.security.revokePasskey')}</Button> : <span className="text-[0.6875rem] text-default-500">{t('users.security.revoked')}</span>}</div>)}</div>}
          </div>
          {resetTwoFactorMutation.isError || revokePasskeyMutation.isError ? <p role="alert" className="text-xs text-danger">{t('users.security.actionError')}</p> : null}
        </div>
      ) : null}
    </section>
  )
}
