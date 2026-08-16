import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Controller, useForm } from 'react-hook-form'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ApiKeyField, ApiKeySwitchField } from '@/features/api-keys/api-key-form-field'
import { ApiKeyIpEditor } from '@/features/api-keys/api-key-ip-editor'
import { ApiKeyModelSelector } from '@/features/api-keys/api-key-model-selector'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import type {
  IssuedOrganizationTokenResponse,
  OrganizationTokenResponse,
} from '@/lib/api/generated/types.gen'
import {
  organizationApiKeyErrorCode,
  useCreateOrganizationApiKey,
  useUpdateOrganizationApiKey,
} from './organization-api-key-api'
import {
  buildOrganizationApiKeyFormSchema,
  defaultOrganizationApiKeyValues,
  toOrganizationApiKeyRequest,
  type OrganizationApiKeyFormValues,
} from './organization-api-key-form-model'

type OrganizationApiKeyFormProps = {
  organizationId: number
  token?: OrganizationTokenResponse
  onCancel: () => void
  onIssued: (issued: IssuedOrganizationTokenResponse) => void
  onSaved: () => void
}

/** 编辑当前成员自己的企业 Key，完整密钥仅从创建成功结果向上交付一次。 */
export function OrganizationApiKeyForm({
  organizationId,
  token,
  onCancel,
  onIssued,
  onSaved,
}: OrganizationApiKeyFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateOrganizationApiKey(organizationId)
  const updateMutation = useUpdateOrganizationApiKey(organizationId)
  const form = useForm<OrganizationApiKeyFormValues>({
    defaultValues: defaultOrganizationApiKeyValues(token),
    resolver: zodResolver(buildOrganizationApiKeyFormSchema({
      invalidField: t('apiKeys.validation.field'),
      invalidModels: t('apiKeys.validation.models'),
      invalidIps: t('apiKeys.validation.ips'),
      invalidDate: t('apiKeys.validation.date'),
    })),
  })

  useEffect(() => form.reset(defaultOrganizationApiKeyValues(token)), [form, token])

  const unlimitedQuota = form.watch('unlimitedQuota')
  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.error ?? updateMutation.error
  const onSubmit = form.handleSubmit(async (values) => {
    try {
      if (token) {
        await updateMutation.mutateAsync({ id: token.id, body: toOrganizationApiKeyRequest(values) })
        onSaved()
      } else {
        const issued = await createMutation.mutateAsync(toOrganizationApiKeyRequest(values))
        createMutation.reset()
        onIssued(issued)
      }
    } catch {
      // 保留输入和一次性密钥边界，失败结果不会关闭编辑器。
    }
  })
  const errorCode = organizationApiKeyErrorCode(submitError)
  const errorKey = errorCode === 'token_limit_reached'
    ? 'organizationApiKeys.form.limitError'
    : errorCode === 'token_outcome_unknown'
      ? 'organizationApiKeys.form.outcomeUnknown'
      : errorCode === 'organization_entitlement_restricted'
        ? 'organizationApiKeys.form.entitlementRestricted'
        : 'organizationApiKeys.form.submitError'

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={onSubmit} noValidate>
      <div className="flex-1 space-y-5 overflow-y-auto px-4 pb-5">
        <section className="grid gap-3" aria-labelledby="organization-api-key-basic-fields">
          <h3 id="organization-api-key-basic-fields" className="text-xs font-semibold">{t('apiKeys.form.basic')}</h3>
          <ApiKeyField id="organization-api-key-name" label={t('apiKeys.fields.name')} error={form.formState.errors.name?.message}>
            <Input id="organization-api-key-name" autoFocus aria-invalid={!!form.formState.errors.name} {...form.register('name')} />
          </ApiKeyField>
          <div className="grid gap-3 sm:grid-cols-2">
            <Controller
              control={form.control}
              name="status"
              render={({ field }) => (
                <ApiKeySwitchField
                  id="organization-api-key-status"
                  label={t('apiKeys.fields.status')}
                  hint={t('organizationApiKeys.form.statusHint')}
                  checked={field.value === 'enabled'}
                  onCheckedChange={(checked) => field.onChange(checked ? 'enabled' : 'disabled')}
                />
              )}
            />
            <ApiKeyField id="organization-api-key-expiry" label={t('apiKeys.fields.expiredAt')} error={form.formState.errors.expiredAt?.message} hint={t('apiKeys.form.expiryHint')}>
              <Input id="organization-api-key-expiry" type="datetime-local" aria-invalid={!!form.formState.errors.expiredAt} {...form.register('expiredAt')} />
            </ApiKeyField>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="organization-api-key-quota-fields">
          <h3 id="organization-api-key-quota-fields" className="text-xs font-semibold">{t('apiKeys.form.quota')}</h3>
          <Controller
            control={form.control}
            name="unlimitedQuota"
            render={({ field }) => (
              <ApiKeySwitchField
                id="organization-api-key-unlimited"
                label={t('apiKeys.fields.unlimitedQuota')}
                hint={t('organizationApiKeys.form.unlimitedHint')}
                checked={field.value}
                onCheckedChange={field.onChange}
              />
            )}
          />
          <div className="grid gap-3 sm:grid-cols-2">
            <ApiKeyField id="organization-api-key-quota" label={t('apiKeys.fields.remainQuota')} error={form.formState.errors.remainQuota?.message} hint={t('organizationApiKeys.form.quotaHint')}>
              <Input id="organization-api-key-quota" type="number" min={0} disabled={unlimitedQuota} aria-invalid={!!form.formState.errors.remainQuota} {...form.register('remainQuota', { valueAsNumber: true })} />
            </ApiKeyField>
            <ApiKeyField id="organization-api-key-max-requests" label={t('organizationApiKeys.fields.maxRequests')} error={form.formState.errors.maxRequests?.message} hint={t('organizationApiKeys.form.optionalLimit')}>
              <Input id="organization-api-key-max-requests" type="number" min={0} aria-invalid={!!form.formState.errors.maxRequests} {...form.register('maxRequests')} />
            </ApiKeyField>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="organization-api-key-policy-fields">
          <h3 id="organization-api-key-policy-fields" className="text-xs font-semibold">{t('apiKeys.form.policy')}</h3>
          <ApiKeyField id="organization-api-key-models" label={t('apiKeys.fields.modelLimits')} error={form.formState.errors.modelLimits?.message} hint={t('apiKeys.form.modelsHint')}>
            <Controller control={form.control} name="modelLimits" render={({ field }) => <ApiKeyModelSelector values={field.value} error={form.formState.errors.modelLimits?.message} onChange={field.onChange} />} />
          </ApiKeyField>
          <ApiKeyField id="organization-api-key-ips" label={t('apiKeys.fields.allowIps')} error={form.formState.errors.allowIps?.message} hint={t('apiKeys.form.ipsHint')}>
            <Controller control={form.control} name="allowIps" render={({ field }) => <ApiKeyIpEditor values={field.value} error={form.formState.errors.allowIps?.message} onChange={field.onChange} />} />
          </ApiKeyField>
        </section>

        <section className="grid gap-3" aria-labelledby="organization-api-key-rate-fields">
          <h3 id="organization-api-key-rate-fields" className="text-xs font-semibold">{t('organizationApiKeys.form.rateLimits')}</h3>
          <div className="grid gap-3 sm:grid-cols-3">
            {(['rateLimit5h', 'rateLimit1d', 'rateLimit7d'] as const).map((field) => (
              <ApiKeyField key={field} id={`organization-api-key-${field}`} label={t(`organizationApiKeys.fields.${field}`)} error={form.formState.errors[field]?.message} hint={t('organizationApiKeys.form.optionalLimit')}>
                <Input id={`organization-api-key-${field}`} type="number" min={0} aria-invalid={!!form.formState.errors[field]} {...form.register(field)} />
              </ApiKeyField>
            ))}
          </div>
        </section>

        {submitError ? (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {t(errorKey)}
          </p>
        ) : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="secondary" disabled={pending} onClick={onCancel}>{t('apiKeys.actions.cancel')}</Button>
        <Button type="submit" disabled={pending}>
          {pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}
          {t(token ? 'apiKeys.actions.save' : 'apiKeys.actions.create')}
        </Button>
      </div>
    </form>
  )
}
