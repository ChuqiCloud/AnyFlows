import { Eye, EyeOff } from 'lucide-react'
import type { ReactNode } from 'react'
import type { UseFormReturn } from 'react-hook-form'
import { Controller } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { NetworkSettingsValues } from './network-settings-form-model'
import { isProxyMode } from './network-settings-form-model'

type NetworkSettingsFieldsProps = {
  form: UseFormReturn<NetworkSettingsValues>
  passwordConfigured: boolean
  passwordVisible: boolean
  onPasswordVisibilityChange: () => void
}

/** 编辑全局代理模式、地址、认证和代理端 DNS 信任开关。 */
export function NetworkSettingsFields({
  form,
  passwordConfigured,
  passwordVisible,
  onPasswordVisibilityChange,
}: NetworkSettingsFieldsProps) {
  const { t } = useTranslation()
  const errors = form.formState.errors
  const mode = form.watch('mode')
  const usesProxy = isProxyMode(mode)

  return (
    <section className="grid gap-4 border-b border-[var(--hairline)] px-5 py-5" aria-labelledby="network-settings-route">
      <div>
        <h3 id="network-settings-route" className="text-sm font-semibold">{t('networkSettings.route.title')}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('networkSettings.route.description')}</p>
      </div>

      <Field id="network-settings-mode" label={t('networkSettings.fields.mode')} hint={t('networkSettings.fields.modeHint')}>
        <Select id="network-settings-mode" {...form.register('mode')}>
          <option value="inherit">{t('networkSettings.modes.inherit')}</option>
          <option value="direct">{t('networkSettings.modes.direct')}</option>
          <option value="http">{t('networkSettings.modes.http')}</option>
          <option value="https">{t('networkSettings.modes.https')}</option>
          <option value="socks5">{t('networkSettings.modes.socks5')}</option>
          <option value="socks5h">{t('networkSettings.modes.socks5h')}</option>
        </Select>
      </Field>

      <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_8rem]">
        <Field id="network-settings-host" label={t('networkSettings.fields.host')} hint={t(usesProxy ? 'networkSettings.fields.hostHint' : 'networkSettings.fields.hostDisabledHint')} error={errors.proxyHost?.message}>
          <Input
            id="network-settings-host"
            disabled={!usesProxy}
            autoCapitalize="none"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={!!errors.proxyHost}
            {...form.register('proxyHost')}
          />
        </Field>
        <Field id="network-settings-port" label={t('networkSettings.fields.port')} error={errors.proxyPort?.message}>
          <Input
            id="network-settings-port"
            type="number"
            min={1}
            max={65_535}
            step={1}
            inputMode="numeric"
            disabled={!usesProxy}
            aria-invalid={!!errors.proxyPort}
            {...form.register('proxyPort', { valueAsNumber: true })}
          />
        </Field>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Field id="network-settings-username" label={t('networkSettings.fields.username')} hint={t(usesProxy ? 'networkSettings.fields.usernameHint' : 'networkSettings.fields.hostDisabledHint')} error={errors.username?.message}>
          <Input
            id="network-settings-username"
            disabled={!usesProxy}
            autoCapitalize="none"
            autoComplete="off"
            spellCheck={false}
            aria-invalid={!!errors.username}
            {...form.register('username')}
          />
        </Field>
        <Field id="network-settings-password" label={t('networkSettings.fields.password')} hint={t(passwordConfigured ? 'networkSettings.fields.passwordKeepHint' : 'networkSettings.fields.passwordEmptyHint')} error={errors.password?.message}>
          <div className="relative">
            <Input
              id="network-settings-password"
              type={passwordVisible ? 'text' : 'password'}
              disabled={!usesProxy}
              autoComplete="new-password"
              className="pr-9"
              aria-invalid={!!errors.password}
              {...form.register('password')}
            />
            <Button
              type="button"
              variant="ghost"
              size="icon-sm"
              disabled={!usesProxy}
              className="absolute right-0.5 top-0.5 size-7 text-muted-foreground"
              aria-label={t(passwordVisible ? 'networkSettings.actions.hidePassword' : 'networkSettings.actions.showPassword')}
              onClick={onPasswordVisibilityChange}
            >
              {passwordVisible ? <EyeOff className="size-3.5" aria-hidden="true" /> : <Eye className="size-3.5" aria-hidden="true" />}
            </Button>
          </div>
        </Field>
      </div>

      <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
        <div>
          <Label htmlFor="network-settings-trust-dns">{t('networkSettings.fields.trustProxyDns')}</Label>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('networkSettings.fields.trustProxyDnsHint')}</p>
        </div>
        <Controller
          control={form.control}
          name="trustProxyDns"
          render={({ field }) => (
            <Switch
              id="network-settings-trust-dns"
              checked={usesProxy && field.value}
              disabled={!usesProxy}
              onCheckedChange={field.onChange}
              aria-label={t('networkSettings.fields.trustProxyDns')}
            />
          )}
        />
      </div>
    </section>
  )
}

function Field({ id, label, hint, error, children }: { id: string; label: string; hint?: string; error?: string; children: ReactNode }) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
