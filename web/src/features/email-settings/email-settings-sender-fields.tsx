import type { UseFormReturn } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Input } from '@/components/ui/input'
import { SettingsField, SettingsSection } from './email-settings-field'
import type { EmailSettingsValues } from './email-settings-form-model'

type EmailSenderFieldsProps = {
  form: UseFormReturn<EmailSettingsValues>
}

/** 编辑发件人身份与可选回复地址。 */
export function EmailSenderFields({ form }: EmailSenderFieldsProps) {
  const { t } = useTranslation()
  const errors = form.formState.errors

  return (
    <SettingsSection
      title={t('emailSettings.sender.title')}
      description={t('emailSettings.sender.description')}
      className="border-t border-[var(--hairline)]"
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <SettingsField
          id="email-settings-from-name"
          label={t('emailSettings.fields.fromName')}
          error={errors.fromName?.message}
        >
          <Input
            id="email-settings-from-name"
            aria-invalid={!!errors.fromName}
            {...form.register('fromName')}
          />
        </SettingsField>
        <SettingsField
          id="email-settings-from-address"
          label={t('emailSettings.fields.fromAddress')}
          error={errors.fromAddress?.message}
        >
          <Input
            id="email-settings-from-address"
            type="email"
            autoCapitalize="none"
            spellCheck={false}
            aria-invalid={!!errors.fromAddress}
            {...form.register('fromAddress')}
          />
        </SettingsField>
      </div>
      <SettingsField
        id="email-settings-reply-to"
        label={t('emailSettings.fields.replyTo')}
        hint={t('emailSettings.fields.replyToHint')}
        error={errors.replyTo?.message}
      >
        <Input
          id="email-settings-reply-to"
          type="email"
          autoCapitalize="none"
          spellCheck={false}
          aria-invalid={!!errors.replyTo}
          {...form.register('replyTo')}
        />
      </SettingsField>
    </SettingsSection>
  )
}
