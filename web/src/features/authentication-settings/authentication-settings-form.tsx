import { zodResolver } from '@hookform/resolvers/zod'
import { Check, LoaderCircle, LockKeyhole, ShieldAlert } from 'lucide-react'
import { type ReactNode, useEffect } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { AdminAuthenticationSettings, AdminGroup } from '@/lib/api/generated/types.gen'
import { useUpdateAdminAuthenticationSettings } from './authentication-settings-api'
import {
  authenticationSettingsValues,
  buildAuthenticationSettingsSchema,
  REGISTRATION_WINDOW_OPTIONS,
  toAuthenticationSettingsRequest,
  type AuthenticationSettingsValues,
} from './authentication-settings-form-model'

type AuthenticationSettingsFormProps = {
  groups: AdminGroup[]
  settings: AdminAuthenticationSettings
}

/** 编辑完整认证设置；关闭密码登录时同步关闭依赖它的公开注册。 */
export function AuthenticationSettingsForm({ groups, settings }: AuthenticationSettingsFormProps) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminAuthenticationSettings()
  const form = useForm<AuthenticationSettingsValues>({
    defaultValues: authenticationSettingsValues(settings),
    resolver: zodResolver(buildAuthenticationSettingsSchema({
      incompatibleCapabilities: t('authenticationSettings.validation.capabilities'),
      invalidGroup: t('authenticationSettings.validation.group'),
      invalidQuota: t('authenticationSettings.validation.quota'),
      invalidRebateQuota: t('authenticationSettings.validation.rebateQuota'),
      invalidAttempts: t('authenticationSettings.validation.attempts'),
      invalidWindow: t('authenticationSettings.validation.window'),
    })),
  })

  useEffect(() => {
    form.reset(authenticationSettingsValues(settings))
  }, [form, settings])

  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const saved = await mutation.mutateAsync(toAuthenticationSettingsRequest(values))
      form.reset(authenticationSettingsValues(saved))
    } catch {
      // 保留管理员输入，便于修正或重试，不回显内部认证诊断。
    }
  })

  const passwordLoginEnabled = form.watch('passwordLoginEnabled')
  const registrationEnabled = form.watch('registrationEnabled')
  const currentWindow = form.watch('rateLimitWindowSeconds')
  const usesCustomWindow = !REGISTRATION_WINDOW_OPTIONS.some(
    (seconds) => String(seconds) === currentWindow,
  )

  return (
    <form id="authentication-core" tabIndex={-1} onSubmit={onSubmit} noValidate className="scroll-mt-20 rounded-2xl outline-none focus-visible:ring-2 focus-visible:ring-ring/60">
      <div className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/45 shadow-[var(--shadow-subtle)]">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--hairline)] px-4 py-3.5">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold">{t('authenticationSettings.status.title')}</h3>
            <Badge className="border-info/25 bg-info/10 text-info">
              {t('authenticationSettings.status.version', { version: settings.version })}
            </Badge>
          </div>
          <LockKeyhole className="size-4 text-info" aria-hidden="true" />
        </div>

        <SettingRow
          title={t('authenticationSettings.passwordLogin.title')}
          description={t('authenticationSettings.passwordLogin.description')}
          control={(
            <div className="flex items-center gap-2.5">
              <StatusBadge
                enabled={passwordLoginEnabled}
                label={t(passwordLoginEnabled
                  ? 'authenticationSettings.status.enabled'
                  : 'authenticationSettings.status.disabled')}
              />
              <Controller
                control={form.control}
                name="passwordLoginEnabled"
                render={({ field }) => (
                  <Switch
                    id="password-login-enabled"
                    checked={field.value}
                    aria-label={t('authenticationSettings.passwordLogin.title')}
                    onCheckedChange={(checked) => {
                      field.onChange(checked)
                      if (!checked && form.getValues('registrationEnabled')) {
                        form.setValue('registrationEnabled', false, {
                          shouldDirty: true,
                          shouldValidate: true,
                        })
                      }
                    }}
                  />
                )}
              />
            </div>
          )}
        />

        <SettingRow
          title={t('authenticationSettings.registration.title')}
          description={t('authenticationSettings.registration.description')}
          control={(
            <div className="flex items-center gap-2.5">
              <StatusBadge
                enabled={registrationEnabled}
                label={t(registrationEnabled
                  ? 'authenticationSettings.status.enabled'
                  : 'authenticationSettings.status.disabled')}
              />
              <Controller
                control={form.control}
                name="registrationEnabled"
                render={({ field }) => (
                  <Switch
                    id="registration-enabled"
                    checked={field.value}
                    disabled={!passwordLoginEnabled}
                    aria-label={t('authenticationSettings.registration.title')}
                    onCheckedChange={field.onChange}
                  />
                )}
              />
            </div>
          )}
        />

        <SettingRow
          title={t('authenticationSettings.email.title')}
          description={t('authenticationSettings.email.description')}
          control={(
            <Controller
              control={form.control}
              name="emailRequired"
              render={({ field }) => (
                <Switch
                  id="registration-email-required"
                  checked={field.value}
                  aria-label={t('authenticationSettings.email.title')}
                  onCheckedChange={field.onChange}
                />
              )}
            />
          )}
        />

        <section id="authentication-allocation" tabIndex={-1} className="grid scroll-mt-20 gap-5 border-b border-[var(--hairline)] px-4 py-5 outline-none focus-visible:ring-2 focus-visible:ring-ring/60 md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]" aria-labelledby="authentication-allocation-title">
          <div>
            <h3 id="authentication-allocation-title" className="text-sm font-semibold">
              {t('authenticationSettings.allocation.title')}
            </h3>
            <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">
              {t('authenticationSettings.allocation.description')}
            </p>
          </div>
          <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
            <SettingsField
              id="authentication-default-group"
              label={t('authenticationSettings.fields.defaultGroup')}
              error={form.formState.errors.defaultGroupId?.message}
            >
              <Select
                id="authentication-default-group"
                aria-invalid={!!form.formState.errors.defaultGroupId}
                {...form.register('defaultGroupId')}
              >
                {groups.map((group) => (
                  <option key={group.id} value={group.id}>
                    {group.display_name} ({group.name})
                  </option>
                ))}
              </Select>
            </SettingsField>
            <SettingsField
              id="authentication-initial-quota"
              label={t('authenticationSettings.fields.initialQuota')}
              hint={t('authenticationSettings.fields.initialQuotaHint')}
              error={form.formState.errors.initialQuota?.message}
            >
              <Input
                id="authentication-initial-quota"
                type="number"
                min={0}
                step={1}
                inputMode="numeric"
                aria-invalid={!!form.formState.errors.initialQuota}
                {...form.register('initialQuota', { valueAsNumber: true })}
              />
            </SettingsField>
            <SettingsField
              id="authentication-invitation-rebate-quota"
              label={t('authenticationSettings.fields.invitationRebateQuota')}
              hint={t('authenticationSettings.fields.invitationRebateQuotaHint')}
              error={form.formState.errors.invitationRebateQuota?.message}
            >
              <Input
                id="authentication-invitation-rebate-quota"
                type="number"
                min={0}
                step={1}
                inputMode="numeric"
                aria-invalid={!!form.formState.errors.invitationRebateQuota}
                {...form.register('invitationRebateQuota', { valueAsNumber: true })}
              />
            </SettingsField>
          </div>
        </section>

        <section id="authentication-protection" tabIndex={-1} className="grid scroll-mt-20 gap-5 px-4 py-5 outline-none focus-visible:ring-2 focus-visible:ring-ring/60 md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]" aria-labelledby="authentication-protection-title">
          <div>
            <h3 id="authentication-protection-title" className="text-sm font-semibold">
              {t('authenticationSettings.protection.title')}
            </h3>
            <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">
              {t('authenticationSettings.protection.description')}
            </p>
          </div>
          <div className="grid gap-4 sm:grid-cols-2">
            <SettingsField
              id="authentication-rate-attempts"
              label={t('authenticationSettings.fields.attempts')}
              error={form.formState.errors.rateLimitAttempts?.message}
            >
              <Input
                id="authentication-rate-attempts"
                type="number"
                min={1}
                max={100}
                step={1}
                inputMode="numeric"
                aria-invalid={!!form.formState.errors.rateLimitAttempts}
                {...form.register('rateLimitAttempts', { valueAsNumber: true })}
              />
            </SettingsField>
            <SettingsField
              id="authentication-rate-window"
              label={t('authenticationSettings.fields.window')}
              error={form.formState.errors.rateLimitWindowSeconds?.message}
            >
              <Select
                id="authentication-rate-window"
                aria-invalid={!!form.formState.errors.rateLimitWindowSeconds}
                {...form.register('rateLimitWindowSeconds')}
              >
                {usesCustomWindow ? (
                  <option value={currentWindow}>
                    {t('authenticationSettings.windows.custom', { seconds: currentWindow })}
                  </option>
                ) : null}
                {REGISTRATION_WINDOW_OPTIONS.map((seconds) => (
                  <option key={seconds} value={seconds}>
                    {t(`authenticationSettings.windows.${seconds}`)}
                  </option>
                ))}
              </Select>
            </SettingsField>
          </div>
        </section>

        <div className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-4 py-3">
          <div className="text-xs">
            {mutation.isError ? (
              <span role="alert" className="text-destructive">{t('authenticationSettings.errors.save')}</span>
            ) : mutation.isSuccess && !form.formState.isDirty ? (
              <span className="inline-flex items-center gap-1.5 text-success">
                <Check className="size-3.5" aria-hidden="true" />
                {t('authenticationSettings.state.saved')}
              </span>
            ) : form.formState.isDirty ? (
              <span className="text-muted-foreground">{t('authenticationSettings.state.unsaved')}</span>
            ) : (
              <span className="text-muted-foreground">{t('authenticationSettings.state.synced')}</span>
            )}
          </div>
          <Button type="submit" disabled={!form.formState.isDirty || mutation.isPending || groups.length === 0}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'authenticationSettings.actions.saving' : 'authenticationSettings.actions.save')}
          </Button>
        </div>
      </div>

      <div className="mt-4 flex items-start gap-2.5 rounded-xl border border-warning/25 bg-warning/8 p-4">
        <ShieldAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden="true" />
        <div>
          <h3 className="text-sm font-semibold">{t('authenticationSettings.warning.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t('authenticationSettings.warning.description')}
          </p>
        </div>
      </div>
    </form>
  )

}

function StatusBadge({ enabled, label }: { enabled: boolean; label: string }) {
  return (
    <Badge className={enabled ? 'border-success/25 bg-success/10 text-success' : undefined}>
      {label}
    </Badge>
  )
}

type SettingRowProps = {
  control: ReactNode
  description: string
  title: string
}

function SettingRow({ control, description, title }: SettingRowProps) {
  return (
    <div className="flex min-h-20 items-center justify-between gap-5 border-b border-[var(--hairline)] px-4 py-4">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{description}</p>
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  )
}

type SettingsFieldProps = {
  children: ReactNode
  error?: string
  hint?: string
  id: string
  label: string
}

function SettingsField({ children, error, hint, id, label }: SettingsFieldProps) {
  return (
    <div className="grid content-start gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
