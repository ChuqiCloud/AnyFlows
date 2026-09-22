import { Button, Switch, Textarea } from '@heroui/react'
import { Controller, type UseFormReturn } from 'react-hook-form'
import { KeyRound, Link2, LockKeyhole } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import { CredentialField, CredentialSecretInput } from './credential-form-field'
import type { CredentialFormValues } from './credential-form-model'

export function CredentialSecretFields({ form, creating, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  creating: boolean
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const kind = form.watch('kind')
  const rotateSecret = form.watch('rotateSecret')
  const oauthCreateMode = form.watch('oauthCreateMode')

  return (
    <div className="grid gap-3">
      {creating && kind === 'oauth' ? (
        <OAuthCreateModePicker form={form} disabled={disabled} />
      ) : null}
      {!creating ? (
        <div className="flex items-start justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
          <div>
            <label htmlFor="credential-rotate-secret" className="text-xs font-medium">{t('credentials.form.rotateSecret')}</label>
            <p className="mt-1 text-[0.6875rem] leading-4 text-muted-foreground">{t('credentials.form.rotateSecretHint')}</p>
          </div>
          <Controller
            control={form.control}
            name="rotateSecret"
            render={({ field }) => (
              <Switch id="credential-rotate-secret" isDisabled={disabled} isSelected={field.value} size="sm" onValueChange={field.onChange} />
            )}
          />
        </div>
      ) : null}

      {!rotateSecret ? (
        <div className="flex items-start gap-2.5 rounded-lg bg-success/8 px-3 py-2.5 text-success">
          <LockKeyhole className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <p className="text-xs leading-5">{t('credentials.form.secretPreserved')}</p>
        </div>
      ) : kind === 'oauth' && creating && oauthCreateMode === 'authorize' ? (
        <div className="flex items-start gap-2.5 rounded-lg bg-info/8 px-3 py-2.5 text-info">
          <Link2 className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          <p className="text-xs leading-5">{t('credentials.secret.authorizeHint')}</p>
        </div>
      ) : kind === 'api_key' ? (
        <CredentialField id="credential-api-key" label={t('credentials.secret.apiKey')} error={form.formState.errors.apiKey?.message}>
          <CredentialSecretInput id="credential-api-key" maxLength={16 * 1_024} disabled={disabled} aria-invalid={Boolean(form.formState.errors.apiKey)} {...form.register('apiKey')} />
        </CredentialField>
      ) : kind === 'oauth' ? (
        <CredentialField id="credential-access-token" label={t('credentials.secret.accessToken')} hint={t('credentials.secret.accessTokenHint')} error={form.formState.errors.accessToken?.message}>
          <CredentialSecretInput id="credential-access-token" maxLength={16 * 1_024} disabled={disabled} aria-invalid={Boolean(form.formState.errors.accessToken)} {...form.register('accessToken')} />
        </CredentialField>
      ) : kind === 'bedrock' ? (
        <BedrockSecretFields form={form} disabled={disabled} />
      ) : (
        <ServiceAccountSecretFields form={form} disabled={disabled} />
      )}
    </div>
  )
}

function OAuthCreateModePicker({ form, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const value = form.watch('oauthCreateMode')
  return (
    <div className="grid gap-1 rounded-lg bg-surface-2 p-1 sm:grid-cols-2" role="radiogroup" aria-label={t('credentials.secret.oauthCreateMode')}>
      {(['authorize', 'access_token'] as const).map((mode) => {
        const Icon = mode === 'authorize' ? Link2 : KeyRound
        return (
          <Button
            key={mode}
            type="button"
            size="sm"
            variant="light"
            role="radio"
            aria-checked={value === mode}
            isDisabled={disabled}
            className={cn('h-10 justify-start text-xs', value === mode && 'bg-background text-foreground hover:bg-background')}
            onClick={() => form.setValue('oauthCreateMode', mode, { shouldDirty: true, shouldValidate: true })}
          >
            <Icon className="size-4" aria-hidden="true" />
            {t(`credentials.secret.oauthMode.${mode}`)}
          </Button>
        )
      })}
    </div>
  )
}

function BedrockSecretFields({ form, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const errors = form.formState.errors
  return (
    <div className="grid gap-3">
      <CredentialField id="credential-access-key-id" label={t('credentials.secret.accessKeyId')} error={errors.accessKeyId?.message}>
        <CredentialSecretInput id="credential-access-key-id" maxLength={128} disabled={disabled} aria-invalid={Boolean(errors.accessKeyId)} {...form.register('accessKeyId')} />
      </CredentialField>
      <CredentialField id="credential-secret-access-key" label={t('credentials.secret.secretAccessKey')} error={errors.secretAccessKey?.message}>
        <CredentialSecretInput id="credential-secret-access-key" maxLength={4 * 1_024} disabled={disabled} aria-invalid={Boolean(errors.secretAccessKey)} {...form.register('secretAccessKey')} />
      </CredentialField>
      <CredentialField id="credential-session-token" label={t('credentials.secret.sessionToken')} hint={t('credentials.secret.optional')} error={errors.sessionToken?.message}>
        <CredentialSecretInput id="credential-session-token" maxLength={16 * 1_024} disabled={disabled} aria-invalid={Boolean(errors.sessionToken)} {...form.register('sessionToken')} />
      </CredentialField>
    </div>
  )
}

function ServiceAccountSecretFields({ form, disabled }: {
  form: UseFormReturn<CredentialFormValues>
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const errors = form.formState.errors
  return (
    <div className="grid gap-3">
      <CredentialField id="credential-client-email" label={t('credentials.secret.clientEmail')} error={errors.clientEmail?.message}>
        <CredentialSecretInput id="credential-client-email" maxLength={320} disabled={disabled} aria-invalid={Boolean(errors.clientEmail)} {...form.register('clientEmail')} />
      </CredentialField>
      <CredentialField id="credential-private-key-id" label={t('credentials.secret.privateKeyId')} hint={t('credentials.secret.optional')} error={errors.privateKeyId?.message}>
        <CredentialSecretInput id="credential-private-key-id" maxLength={128} disabled={disabled} aria-invalid={Boolean(errors.privateKeyId)} {...form.register('privateKeyId')} />
      </CredentialField>
      <CredentialField id="credential-private-key" label={t('credentials.secret.privateKey')} hint={t('credentials.secret.privateKeyHint')} error={errors.privateKey?.message}>
        <div className="relative">
          <KeyRound className="pointer-events-none absolute top-2.5 left-3 size-4 text-muted-foreground" aria-hidden="true" />
          <Textarea id="credential-private-key" classNames={{ input: 'resize-y pl-9 font-mono text-xs' }} isDisabled={disabled} isInvalid={Boolean(errors.privateKey)} maxLength={16 * 1_024} minRows={8} size="sm" spellCheck="false" {...form.register('privateKey')} />
        </div>
      </CredentialField>
    </div>
  )
}
