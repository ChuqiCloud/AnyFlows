import { zodResolver } from '@hookform/resolvers/zod'
import { Check, Globe2, LoaderCircle, LockKeyhole } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { AdminNetworkSettings } from '@/lib/api/generated/types.gen'
import { useUpdateAdminNetworkSettings } from './network-settings-api'
import { buildNetworkSettingsSchema, networkSettingsValues, toNetworkSettingsRequest, type NetworkSettingsValues } from './network-settings-form-model'
import { NetworkSettingsFields } from './network-settings-fields'

type NetworkSettingsWorkspaceProps = {
  settings: AdminNetworkSettings
}

/** 将网络路由编辑、脱敏状态和安全边界集中呈现为一个可保存工作区。 */
export function NetworkSettingsWorkspace({ settings }: NetworkSettingsWorkspaceProps) {
  const { t } = useTranslation()
  const mutation = useUpdateAdminNetworkSettings()
  const [passwordVisible, setPasswordVisible] = useState(false)
  const form = useForm<NetworkSettingsValues>({
    defaultValues: networkSettingsValues(settings),
    resolver: zodResolver(buildNetworkSettingsSchema(settings.password_configured, {
      host: t('networkSettings.validation.host'),
      port: t('networkSettings.validation.port'),
      username: t('networkSettings.validation.username'),
      password: t('networkSettings.validation.password'),
      passwordRequired: t('networkSettings.validation.passwordRequired'),
      passwordWithoutUser: t('networkSettings.validation.passwordWithoutUser'),
    })),
  })

  useEffect(() => {
    form.reset(networkSettingsValues(settings))
    setPasswordVisible(false)
  }, [form, settings])

  const onSubmit = form.handleSubmit(async (values) => {
    try {
      const saved = await mutation.mutateAsync(toNetworkSettingsRequest(values))
      form.reset(networkSettingsValues(saved))
      setPasswordVisible(false)
    } catch {
      // 保留管理员输入，错误只展示固定通用文案。
    }
  })

  const mode = form.watch('mode')
  const modeLabel = t(`networkSettings.modes.${mode}`)

  return (
    <form className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_19rem]" onSubmit={onSubmit} noValidate>
      <div className="overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1/55 shadow-[var(--shadow-subtle)]">
        <div className="flex flex-col gap-3 border-b border-[var(--hairline)] px-5 py-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex items-start gap-3">
            <div className="grid size-9 shrink-0 place-items-center rounded-lg bg-brand/10 text-brand">
              <Globe2 className="size-4" aria-hidden="true" />
            </div>
            <div>
              <div className="flex flex-wrap items-center gap-2">
                <h3 className="text-sm font-semibold">{t('networkSettings.status.title')}</h3>
                <Badge className="border-info/25 bg-info/10 text-info">{modeLabel}</Badge>
                {settings.password_configured ? <Badge className="border-success/25 bg-success/10 text-success">{t('networkSettings.status.passwordConfigured')}</Badge> : null}
              </div>
              <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('networkSettings.status.version', { version: settings.version })}</p>
            </div>
          </div>
        </div>

        <NetworkSettingsFields
          form={form}
          passwordConfigured={settings.password_configured}
          passwordVisible={passwordVisible}
          onPasswordVisibilityChange={() => setPasswordVisible((visible) => !visible)}
        />

        <div className="flex min-h-14 flex-wrap items-center justify-between gap-3 border-t border-[var(--hairline)] bg-surface-2/25 px-5 py-3">
          <div className="text-xs">
            {mutation.isError ? <span role="alert" className="text-destructive">{t('networkSettings.errors.save')}</span> : mutation.isSuccess && !form.formState.isDirty ? (
              <span className="inline-flex items-center gap-1.5 text-success"><Check className="size-3.5" aria-hidden="true" />{t('networkSettings.state.saved')}</span>
            ) : form.formState.isDirty ? <span className="text-muted-foreground">{t('networkSettings.state.unsaved')}</span> : <span className="text-muted-foreground">{t('networkSettings.state.synced')}</span>}
          </div>
          <Button type="submit" disabled={!form.formState.isDirty || mutation.isPending}>
            {mutation.isPending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
            {t(mutation.isPending ? 'networkSettings.actions.saving' : 'networkSettings.actions.save')}
          </Button>
        </div>
      </div>

      <aside className="grid gap-4">
        <section className="rounded-xl border border-info/20 bg-info/8 p-4">
          <div className="flex items-start gap-2.5">
            <LockKeyhole className="mt-0.5 size-4 shrink-0 text-info" aria-hidden="true" />
            <div>
              <h3 className="text-sm font-semibold">{t('networkSettings.security.title')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('networkSettings.security.description')}</p>
            </div>
          </div>
        </section>
        <section className="rounded-xl border border-[var(--hairline)] bg-surface-1/45 p-4">
          <h3 className="text-sm font-semibold">{t('networkSettings.behavior.title')}</h3>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('networkSettings.behavior.description')}</p>
        </section>
      </aside>
    </form>
  )
}
