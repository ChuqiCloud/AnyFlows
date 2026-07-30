import { zodResolver } from '@hookform/resolvers/zod'
import { Check, LoaderCircle } from 'lucide-react'
import { type ReactNode, useEffect } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { AdminGroup, RegistrationPolicyResponse } from '@/lib/api/generated/types.gen'
import { useUpdateAdminRegistrationPolicy } from './registration-api'
import {
  buildRegistrationSettingsSchema,
  REGISTRATION_WINDOW_OPTIONS,
  settingsValuesFromPolicy,
  toRegistrationPolicyRequest,
  type RegistrationSettingsValues,
} from './registration-form-model'

type RegistrationSettingsFormProps = {
  groups: AdminGroup[]
  policy: RegistrationPolicyResponse
}

/** 使用真实分组目录编辑完整注册策略，保存时不发送局部或猜测值。 */
export function RegistrationSettingsForm({ groups, policy }: RegistrationSettingsFormProps) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminRegistrationPolicy()
  const form = useForm<RegistrationSettingsValues>({
    defaultValues: settingsValuesFromPolicy(policy),
    resolver: zodResolver(buildRegistrationSettingsSchema({
      invalidGroup: t('registration.settings.validation.group'),
      invalidQuota: t('registration.settings.validation.quota'),
      invalidAttempts: t('registration.settings.validation.attempts'),
      invalidWindow: t('registration.settings.validation.window'),
    })),
  })

  useEffect(() => {
    form.reset(settingsValuesFromPolicy(policy))
  }, [form, policy])

  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const saved = await mutation.mutateAsync(toRegistrationPolicyRequest(values))
      form.reset(settingsValuesFromPolicy(saved))
    } catch {
      // Mutation 保留结构化失败状态，管理员输入保持原样以便修正或重试。
    }
  })

  const enabled = form.watch('enabled')
  const currentWindow = form.watch('rateLimitWindowSeconds')
  const usesCustomWindow = !REGISTRATION_WINDOW_OPTIONS.some(
    (seconds) => String(seconds) === currentWindow,
  )

  return (
    <form onSubmit={onSubmit} noValidate>
      <div className="border-y border-[var(--hairline)]">
        <SettingRow
          title={t('registration.settings.access.title')}
          description={t('registration.settings.access.description')}
          control={(
            <div className="flex items-center gap-2.5">
              <Badge className={enabled ? 'border-success/25 bg-success/10 text-success' : undefined}>
                {t(enabled ? 'registration.settings.status.enabled' : 'registration.settings.status.disabled')}
              </Badge>
              <Controller
                control={form.control}
                name="enabled"
                render={({ field }) => (
                  <Switch
                    id="registration-enabled"
                    checked={field.value}
                    aria-label={t('registration.settings.access.title')}
                    onCheckedChange={(checked) => field.onChange(checked)}
                  />
                )}
              />
            </div>
          )}
        />

        <SettingRow
          title={t('registration.settings.email.title')}
          description={t('registration.settings.email.description')}
          control={(
            <Controller
              control={form.control}
              name="emailRequired"
              render={({ field }) => (
                <Switch
                  id="registration-email-required"
                  checked={field.value}
                  aria-label={t('registration.settings.email.title')}
                  onCheckedChange={(checked) => field.onChange(checked)}
                />
              )}
            />
          )}
        />

        <section className="grid gap-5 py-5 md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]" aria-labelledby="registration-allocation-title">
          <div>
            <h3 id="registration-allocation-title" className="text-sm font-semibold">
              {t('registration.settings.allocation.title')}
            </h3>
            <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">
              {t('registration.settings.allocation.description')}
            </p>
          </div>
          <div className="grid gap-4 sm:grid-cols-2">
            <SettingsField
              id="registration-default-group"
              label={t('registration.settings.fields.defaultGroup')}
              error={form.formState.errors.defaultGroupId?.message}
            >
              <Select
                id="registration-default-group"
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
              id="registration-initial-quota"
              label={t('registration.settings.fields.initialQuota')}
              hint={t('registration.settings.fields.initialQuotaHint')}
              error={form.formState.errors.initialQuota?.message}
            >
              <Input
                id="registration-initial-quota"
                type="number"
                min={0}
                step={1}
                inputMode="numeric"
                aria-invalid={!!form.formState.errors.initialQuota}
                {...form.register('initialQuota', { valueAsNumber: true })}
              />
            </SettingsField>
          </div>
        </section>

        <section className="grid gap-5 py-5 md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]" aria-labelledby="registration-protection-title">
          <div>
            <h3 id="registration-protection-title" className="text-sm font-semibold">
              {t('registration.settings.protection.title')}
            </h3>
            <p className="mt-1 max-w-md text-xs leading-5 text-muted-foreground">
              {t('registration.settings.protection.description')}
            </p>
          </div>
          <div className="grid gap-4 sm:grid-cols-2">
            <SettingsField
              id="registration-rate-attempts"
              label={t('registration.settings.fields.attempts')}
              error={form.formState.errors.rateLimitAttempts?.message}
            >
              <Input
                id="registration-rate-attempts"
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
              id="registration-rate-window"
              label={t('registration.settings.fields.window')}
              error={form.formState.errors.rateLimitWindowSeconds?.message}
            >
              <Select
                id="registration-rate-window"
                aria-invalid={!!form.formState.errors.rateLimitWindowSeconds}
                {...form.register('rateLimitWindowSeconds')}
              >
                {usesCustomWindow ? (
                  <option value={currentWindow}>
                    {t('registration.settings.windows.custom', { seconds: currentWindow })}
                  </option>
                ) : null}
                {REGISTRATION_WINDOW_OPTIONS.map((seconds) => (
                  <option key={seconds} value={seconds}>
                    {t(`registration.settings.windows.${seconds}`)}
                  </option>
                ))}
              </Select>
            </SettingsField>
          </div>
        </section>
      </div>

      <div className="mt-4 flex min-h-9 flex-wrap items-center justify-between gap-3">
        <div className="text-xs">
          {mutation.isError ? (
            <span role="alert" className="text-destructive">{t('registration.settings.errors.save')}</span>
          ) : mutation.isSuccess && !form.formState.isDirty ? (
            <span className="inline-flex items-center gap-1.5 text-success">
              <Check className="size-3.5" aria-hidden="true" />
              {t('registration.settings.state.saved')}
            </span>
          ) : form.formState.isDirty ? (
            <span className="text-muted-foreground">{t('registration.settings.state.unsaved')}</span>
          ) : null}
        </div>
        <Button
          type="submit"
          disabled={!form.formState.isDirty || mutation.isPending || groups.length === 0}
        >
          {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(mutation.isPending ? 'registration.settings.actions.saving' : 'registration.settings.actions.save')}
        </Button>
      </div>
    </form>
  )
}

type SettingRowProps = {
  title: string
  description: string
  control: ReactNode
}

function SettingRow({ title, description, control }: SettingRowProps) {
  return (
    <div className="flex min-h-20 items-center justify-between gap-5 border-b border-[var(--hairline)] py-4">
      <div>
        <h3 className="text-sm font-semibold">{title}</h3>
        <p className="mt-1 max-w-2xl text-xs leading-5 text-muted-foreground">{description}</p>
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  )
}

type SettingsFieldProps = {
  id: string
  label: string
  hint?: string
  error?: string
  children: ReactNode
}

function SettingsField({ id, label, hint, error, children }: SettingsFieldProps) {
  return (
    <div className="grid content-start gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
