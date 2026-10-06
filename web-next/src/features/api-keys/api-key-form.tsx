import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Input } from '@heroui/react'
import { Controller, useForm } from 'react-hook-form'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { IssuedUserToken, UserToken } from '@/lib/api/generated/types.gen'
import { hasManagementErrorCode, useCreateApiKey, useUpdateApiKey } from './api-key-api'
import { ApiKeyField, ApiKeySwitchField } from './api-key-form-field'
import {
  buildApiKeyFormSchema,
  defaultApiKeyValues,
  toApiKeyRequest,
  type ApiKeyFormValues,
} from './api-key-form-model'
import { ApiKeyIpEditor } from './api-key-ip-editor'
import { ApiKeyModelSelector } from './api-key-model-selector'

type ApiKeyFormProps = {
  token?: UserToken
  onCancel: () => void
  onIssued: (issued: IssuedUserToken) => void
  onSaved: () => void
}

export function ApiKeyForm({ token, onCancel, onIssued, onSaved }: ApiKeyFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateApiKey()
  const updateMutation = useUpdateApiKey()
  const form = useForm<ApiKeyFormValues>({
    defaultValues: defaultApiKeyValues(token),
    resolver: zodResolver(buildApiKeyFormSchema({
      invalidField: t('apiKeys.validation.field'),
      invalidModels: t('apiKeys.validation.models'),
      invalidIps: t('apiKeys.validation.ips'),
      invalidDate: t('apiKeys.validation.date'),
    })),
  })

  useEffect(() => form.reset(defaultApiKeyValues(token)), [form, token])

  const unlimitedQuota = form.watch('unlimitedQuota')
  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.error ?? updateMutation.error
  const limitReached = hasManagementErrorCode(submitError, 'token_limit_reached')
  const onSubmit = form.handleSubmit(async (values) => {
    try {
      if (token) {
        await updateMutation.mutateAsync({ id: token.id, body: toApiKeyRequest(values) })
        onSaved()
      } else {
        const issued = await createMutation.mutateAsync(toApiKeyRequest(values))
        createMutation.reset()
        onIssued(issued)
      }
    } catch {
      // 保留表单输入；一次性密钥只有成功结果会交给临时展示面板。
    }
  })

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={onSubmit} noValidate>
      <div className="flex-1 space-y-5 overflow-y-auto px-4 pt-4 pb-5">
        <section className="grid gap-3" aria-labelledby="api-key-basic-fields">
          <h3 id="api-key-basic-fields" className="text-xs font-semibold">{t('apiKeys.form.basic')}</h3>
          <ApiKeyField id="api-key-name" label={t('apiKeys.fields.name')} error={form.formState.errors.name?.message}>
            <Input autoFocus id="api-key-name" isInvalid={!!form.formState.errors.name} size="sm" {...form.register('name')} />
          </ApiKeyField>
          <div className="grid gap-3 sm:grid-cols-2">
            <Controller
              control={form.control}
              name="status"
              render={({ field }) => (
                <ApiKeySwitchField
                  id="api-key-status"
                  label={t('apiKeys.fields.status')}
                  hint={t('apiKeys.form.statusHint')}
                  checked={field.value === 'enabled'}
                  onCheckedChange={(checked) => field.onChange(checked ? 'enabled' : 'disabled')}
                />
              )}
            />
            <ApiKeyField id="api-key-expiry" label={t('apiKeys.fields.expiredAt')} error={form.formState.errors.expiredAt?.message} hint={t('apiKeys.form.expiryHint')}>
              <Input id="api-key-expiry" isInvalid={!!form.formState.errors.expiredAt} size="sm" type="datetime-local" {...form.register('expiredAt')} />
            </ApiKeyField>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="api-key-quota-fields">
          <h3 id="api-key-quota-fields" className="text-xs font-semibold">{t('apiKeys.form.quota')}</h3>
          <Controller
            control={form.control}
            name="unlimitedQuota"
            render={({ field }) => (
              <ApiKeySwitchField
                id="api-key-unlimited"
                label={t('apiKeys.fields.unlimitedQuota')}
                hint={t('apiKeys.form.unlimitedHint')}
                checked={field.value}
                onCheckedChange={field.onChange}
              />
            )}
          />
          <ApiKeyField id="api-key-quota" label={t('apiKeys.fields.remainQuota')} error={form.formState.errors.remainQuota?.message} hint={t('apiKeys.form.quotaHint')}>
            <Input id="api-key-quota" inputMode="numeric" isDisabled={unlimitedQuota} isInvalid={!!form.formState.errors.remainQuota} min={0} size="sm" type="number" {...form.register('remainQuota', { valueAsNumber: true })} />
          </ApiKeyField>
        </section>

        <section className="grid gap-3" aria-labelledby="api-key-policy-fields">
          <h3 id="api-key-policy-fields" className="text-xs font-semibold">{t('apiKeys.form.policy')}</h3>
          <ApiKeyField id="api-key-models" label={t('apiKeys.fields.modelLimits')} error={form.formState.errors.modelLimits?.message} hint={t('apiKeys.form.modelsHint')}>
            <Controller control={form.control} name="modelLimits" render={({ field }) => <ApiKeyModelSelector values={field.value} error={form.formState.errors.modelLimits?.message} onChange={field.onChange} />} />
          </ApiKeyField>
          <ApiKeyField id="api-key-ips" label={t('apiKeys.fields.allowIps')} error={form.formState.errors.allowIps?.message} hint={t('apiKeys.form.ipsHint')}>
            <Controller control={form.control} name="allowIps" render={({ field }) => <ApiKeyIpEditor values={field.value} error={form.formState.errors.allowIps?.message} onChange={field.onChange} />} />
          </ApiKeyField>
        </section>

        {submitError ? (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {t(limitReached ? 'apiKeys.form.limitError' : 'apiKeys.form.submitError')}
          </p>
        ) : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="bordered" isDisabled={pending} onClick={onCancel}>{t('apiKeys.actions.cancel')}</Button>
        <Button color="primary" type="submit" isDisabled={pending}>
          {pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
          {t(token ? 'apiKeys.actions.save' : 'apiKeys.actions.create')}
        </Button>
      </div>
    </form>
  )
}
