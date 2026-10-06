import { Button, Input, Select, SelectItem } from '@heroui/react'
import { Eye, EyeOff } from 'lucide-react'
import type { UseFormReturn } from 'react-hook-form'
import { Controller } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { SettingsField, SettingsSection } from './email-settings-field'
import type { EmailSettingsValues } from './email-settings-form-model'

type EmailSettingsFieldsProps = {
  form: UseFormReturn<EmailSettingsValues>
  passwordConfigured: boolean
  passwordVisible: boolean
  onPasswordVisibilityChange: () => void
}

/** 编辑 SMTP 连接、TLS、认证与硬超时。 */
export function EmailConnectionFields({
  form,
  passwordConfigured,
  passwordVisible,
  onPasswordVisibilityChange,
}: EmailSettingsFieldsProps) {
  const { t } = useTranslation()
  const errors = form.formState.errors

  return (
    <SettingsSection
      title={t('emailSettings.connection.title')}
      description={t('emailSettings.connection.description')}
    >
      <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_8rem]">
        <SettingsField
          id="email-settings-host"
          label={t('emailSettings.fields.host')}
          hint={t('emailSettings.fields.hostHint')}
          error={errors.host?.message}
        >
          <Input
            id="email-settings-host"
            autoCapitalize="none"
            autoComplete="off"
            isInvalid={!!errors.host}
            size="sm"
            spellCheck="false"
            {...form.register('host')}
          />
        </SettingsField>
        <SettingsField
          id="email-settings-port"
          label={t('emailSettings.fields.port')}
          error={errors.port?.message}
        >
          <Input
            id="email-settings-port"
            type="number"
            min={1}
            max={65_535}
            step={1}
            inputMode="numeric"
            isInvalid={!!errors.port}
            size="sm"
            {...form.register('port', { valueAsNumber: true })}
          />
        </SettingsField>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <SettingsField
          id="email-settings-tls"
          label={t('emailSettings.fields.tlsMode')}
          hint={t('emailSettings.fields.tlsHint')}
        >
          <Controller
            control={form.control}
            name="tlsMode"
            render={({ field }) => (
              <Select
                aria-label={t('emailSettings.fields.tlsMode')}
                id="email-settings-tls"
                selectedKeys={[field.value]}
                size="sm"
                onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? 'start_tls'))}
              >
                <SelectItem key="start_tls">{t('emailSettings.tls.startTls')}</SelectItem>
                <SelectItem key="tls">{t('emailSettings.tls.tls')}</SelectItem>
              </Select>
            )}
          />
        </SettingsField>
        <SettingsField
          id="email-settings-timeout"
          label={t('emailSettings.fields.timeout')}
          hint={t('emailSettings.fields.timeoutHint')}
          error={errors.timeoutSeconds?.message}
        >
          <Input
            id="email-settings-timeout"
            type="number"
            min={1}
            max={60}
            step={1}
            inputMode="numeric"
            isInvalid={!!errors.timeoutSeconds}
            size="sm"
            {...form.register('timeoutSeconds', { valueAsNumber: true })}
          />
        </SettingsField>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <SettingsField
          id="email-settings-username"
          label={t('emailSettings.fields.username')}
          hint={t('emailSettings.fields.usernameHint')}
          error={errors.username?.message}
        >
          <Input
            id="email-settings-username"
            autoCapitalize="none"
            autoComplete="off"
            isInvalid={!!errors.username}
            size="sm"
            spellCheck="false"
            {...form.register('username')}
          />
        </SettingsField>
        <SettingsField
          id="email-settings-password"
          label={t('emailSettings.fields.password')}
          hint={t(passwordConfigured
            ? 'emailSettings.fields.passwordKeepHint'
            : 'emailSettings.fields.passwordEmptyHint')}
          error={errors.password?.message}
        >
          <div className="relative">
            <Input
              id="email-settings-password"
              type={passwordVisible ? 'text' : 'password'}
              autoComplete="new-password"
              isInvalid={!!errors.password}
              size="sm"
              classNames={{ inputWrapper: 'pr-9' }}
              {...form.register('password')}
            />
            <Button
              type="button"
              isIconOnly
              size="sm"
              variant="light"
              className="absolute right-0.5 top-0.5 size-7 min-w-7 text-muted-foreground"
              aria-label={t(passwordVisible
                ? 'emailSettings.actions.hidePassword'
                : 'emailSettings.actions.showPassword')}
              onClick={onPasswordVisibilityChange}
            >
              {passwordVisible
                ? <EyeOff className="size-3.5" aria-hidden="true" />
                : <Eye className="size-3.5" aria-hidden="true" />}
            </Button>
          </div>
        </SettingsField>
      </div>
    </SettingsSection>
  )
}
