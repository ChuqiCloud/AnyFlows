import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Chip, Switch } from '@heroui/react'
import { Check, LoaderCircle, LockKeyhole } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import type { AdminEmailSettings } from '@/lib/api/generated/types.gen'
import { useUpdateAdminEmailSettings } from './email-settings-api'
import { EmailConnectionFields } from './email-settings-fields'
import {
  buildEmailSettingsSchema,
  emailSettingsValues,
  toEmailSettingsRequest,
  type EmailSettingsValues,
} from './email-settings-form-model'
import { EmailTestPanel } from './email-test-panel'
import { EmailSenderFields } from './email-settings-sender-fields'

type EmailSettingsWorkspaceProps = {
  settings: AdminEmailSettings
}

/** 将完整设置保存与测试投递并列呈现，测试始终使用最近一次已保存快照。 */
export function EmailSettingsWorkspace({ settings }: EmailSettingsWorkspaceProps) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminEmailSettings()
  const [passwordVisible, setPasswordVisible] = useState(false)
  const form = useForm<EmailSettingsValues>({
    defaultValues: emailSettingsValues(settings),
    resolver: zodResolver(buildEmailSettingsSchema(settings.password_configured, {
      host: t('emailSettings.validation.host'),
      port: t('emailSettings.validation.port'),
      username: t('emailSettings.validation.username'),
      password: t('emailSettings.validation.password'),
      passwordRequired: t('emailSettings.validation.passwordRequired'),
      passwordWithoutUser: t('emailSettings.validation.passwordWithoutUser'),
      fromAddress: t('emailSettings.validation.fromAddress'),
      fromAddressRequired: t('emailSettings.validation.fromAddressRequired'),
      fromName: t('emailSettings.validation.fromName'),
      replyTo: t('emailSettings.validation.replyTo'),
      timeout: t('emailSettings.validation.timeout'),
    })),
  })

  useEffect(() => {
    form.reset(emailSettingsValues(settings))
    setPasswordVisible(false)
  }, [form, settings])

  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const saved = await mutation.mutateAsync(toEmailSettingsRequest(values))
      form.reset(emailSettingsValues(saved))
      setPasswordVisible(false)
    } catch {
      // 保留管理员输入，错误只展示固定通用文案，不回显 SMTP 诊断。
    }
  })

  const enabled = form.watch('enabled')
  const username = form.watch('username')
  const password = form.watch('password')
  const passwordState = !username
    ? 'disabled'
    : password
      ? 'replace'
      : settings.password_configured
        ? 'configured'
        : 'missing'

  return (
    <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1.45fr)_minmax(16rem,0.75fr)]">
      <form
        className="min-w-0 overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/55"
        onSubmit={onSubmit}
        noValidate
      >
        <div className="flex flex-col gap-3 border-b border-[var(--hairline)] px-4 py-3.5 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="text-sm font-semibold">{t('emailSettings.status.title')}</h3>
              <Chip className={enabled ? 'bg-success/10 text-success' : undefined} size="sm" variant="flat">
                {t(enabled ? 'emailSettings.status.enabled' : 'emailSettings.status.disabled')}
              </Chip>
              <Chip className={settings.delivery_ready ? 'bg-info/10 text-info' : undefined} size="sm" variant="flat">
                {t(settings.delivery_ready
                  ? 'emailSettings.status.ready'
                  : 'emailSettings.status.incomplete')}
              </Chip>
            </div>
            <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">
              {t('emailSettings.status.version', { version: settings.version })}
            </p>
          </div>
          <div className="flex items-center justify-between gap-3 sm:justify-end">
            <span className="text-xs text-muted-foreground">{t('emailSettings.status.delivery')}</span>
            <Controller
              control={form.control}
              name="enabled"
              render={({ field }) => (
                <Switch
                  aria-label={t('emailSettings.status.delivery')}
                  isSelected={field.value}
                  size="sm"
                  onValueChange={field.onChange}
                />
              )}
            />
          </div>
        </div>

        <EmailConnectionFields
          form={form}
          passwordConfigured={settings.password_configured}
          passwordVisible={passwordVisible}
          onPasswordVisibilityChange={() => setPasswordVisible((visible) => !visible)}
        />
        <EmailSenderFields form={form} />

        <div className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-4 py-3">
          <div className="text-xs">
            {mutation.isError ? (
              <span role="alert" className="text-destructive">{t('emailSettings.errors.save')}</span>
            ) : mutation.isSuccess && !form.formState.isDirty ? (
              <span className="inline-flex items-center gap-1.5 text-success">
                <Check className="size-3.5" aria-hidden="true" />
                {t('emailSettings.state.saved')}
              </span>
            ) : form.formState.isDirty ? (
              <span className="text-muted-foreground">{t('emailSettings.state.unsaved')}</span>
            ) : (
              <span className="text-muted-foreground">
                {t(`emailSettings.passwordState.${passwordState}`)}
              </span>
            )}
          </div>
          <Button type="submit" color="primary" isDisabled={!form.formState.isDirty || mutation.isPending}>
            {mutation.isPending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'emailSettings.actions.saving' : 'emailSettings.actions.save')}
          </Button>
        </div>
      </form>

      <aside className="grid gap-4">
        <EmailTestPanel
          key={settings.version}
          settings={settings}
          settingsDirty={form.formState.isDirty}
        />
        <section className="rounded-xl border border-info/20 bg-info/8 p-4">
          <div className="flex items-start gap-2.5">
            <LockKeyhole className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
            <div>
              <h3 className="text-sm font-semibold">{t('emailSettings.security.title')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">
                {t('emailSettings.security.description')}
              </p>
            </div>
          </div>
        </section>
      </aside>
    </div>
  )
}
