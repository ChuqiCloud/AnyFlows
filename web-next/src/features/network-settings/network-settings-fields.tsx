import { Button, Input, Select, SelectItem, Switch } from '@heroui/react'
import { Eye, EyeOff } from 'lucide-react'
import type { ReactNode } from 'react'
import type { UseFormReturn } from 'react-hook-form'
import { Controller } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import type { NetworkSettingsValues } from './network-settings-form-model'
import { isProxyMode } from './network-settings-form-model'

/** 网络代理模式的可选值，顺序与原原生选项一致。 */
const MODE_KEYS = ['inherit', 'direct', 'http', 'https', 'socks5', 'socks5h'] as const

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
  const modeItems = MODE_KEYS.map((key) => ({ key, label: t(`networkSettings.modes.${key}`) }))

  return (
    <section className="grid gap-4 border-b border-[var(--hairline)] px-5 py-5" aria-labelledby="network-settings-route">
      <div>
        <h3 id="network-settings-route" className="text-sm font-semibold">{t('networkSettings.route.title')}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('networkSettings.route.description')}</p>
      </div>

      <Field id="network-settings-mode" label={t('networkSettings.fields.mode')} hint={t('networkSettings.fields.modeHint')}>
        <Controller
          control={form.control}
          name="mode"
          render={({ field }) => (
            <Select
              aria-label={t('networkSettings.fields.mode')}
              id="network-settings-mode"
              items={modeItems}
              selectedKeys={[field.value]}
              size="sm"
              onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? 'inherit'))}
            >
              {(item) => <SelectItem key={item.key}>{item.label}</SelectItem>}
            </Select>
          )}
        />
      </Field>

      <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_8rem]">
        <Field id="network-settings-host" label={t('networkSettings.fields.host')} hint={t(usesProxy ? 'networkSettings.fields.hostHint' : 'networkSettings.fields.hostDisabledHint')} error={errors.proxyHost?.message}>
          <Input
            id="network-settings-host"
            autoCapitalize="none"
            autoComplete="off"
            isDisabled={!usesProxy}
            isInvalid={!!errors.proxyHost}
            size="sm"
            spellCheck="false"
            {...form.register('proxyHost')}
          />
        </Field>
        <Field id="network-settings-port" label={t('networkSettings.fields.port')} error={errors.proxyPort?.message}>
          <Input
            id="network-settings-port"
            inputMode="numeric"
            isDisabled={!usesProxy}
            isInvalid={!!errors.proxyPort}
            max={65_535}
            min={1}
            size="sm"
            step={1}
            type="number"
            {...form.register('proxyPort', { valueAsNumber: true })}
          />
        </Field>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <Field id="network-settings-username" label={t('networkSettings.fields.username')} hint={t(usesProxy ? 'networkSettings.fields.usernameHint' : 'networkSettings.fields.hostDisabledHint')} error={errors.username?.message}>
          <Input
            id="network-settings-username"
            autoCapitalize="none"
            autoComplete="off"
            isDisabled={!usesProxy}
            isInvalid={!!errors.username}
            size="sm"
            spellCheck="false"
            {...form.register('username')}
          />
        </Field>
        <Field id="network-settings-password" label={t('networkSettings.fields.password')} hint={t(passwordConfigured ? 'networkSettings.fields.passwordKeepHint' : 'networkSettings.fields.passwordEmptyHint')} error={errors.password?.message}>
          <div className="relative">
            <Input
              className="pr-9"
              id="network-settings-password"
              autoComplete="new-password"
              isDisabled={!usesProxy}
              isInvalid={!!errors.password}
              size="sm"
              type={passwordVisible ? 'text' : 'password'}
              {...form.register('password')}
            />
            <Button
              isIconOnly
              aria-label={t(passwordVisible ? 'networkSettings.actions.hidePassword' : 'networkSettings.actions.showPassword')}
              className="absolute right-0.5 top-0.5 size-7 min-w-7 text-muted-foreground"
              isDisabled={!usesProxy}
              size="sm"
              type="button"
              variant="light"
              onClick={onPasswordVisibilityChange}
            >
              {passwordVisible ? <EyeOff className="size-3.5" aria-hidden="true" /> : <Eye className="size-3.5" aria-hidden="true" />}
            </Button>
          </div>
        </Field>
      </div>

      <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
        <div>
          <label className="text-xs font-medium leading-none text-foreground" htmlFor="network-settings-trust-dns">{t('networkSettings.fields.trustProxyDns')}</label>
          <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('networkSettings.fields.trustProxyDnsHint')}</p>
        </div>
        <Controller
          control={form.control}
          name="trustProxyDns"
          render={({ field }) => (
            <Switch
              aria-label={t('networkSettings.fields.trustProxyDns')}
              id="network-settings-trust-dns"
              isDisabled={!usesProxy}
              isSelected={usesProxy && field.value}
              size="sm"
              onValueChange={field.onChange}
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
      {/* HeroUI 没有独立的 Label 组件，表单标签保留原生元素以维持原有的标签/提示/错误层级。 */}
      <label className="text-xs font-medium leading-none text-foreground" htmlFor={id}>{label}</label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}
