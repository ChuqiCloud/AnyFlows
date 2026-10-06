import type { ReactNode } from 'react'
import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Controller, useForm } from 'react-hook-form'
import { LoaderCircle } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import { Textarea } from '@/components/ui/textarea'
import type { AdminToken, IssuedAdminToken } from '@/lib/api/generated/types.gen'
import { useCreateAdminToken, useUpdateAdminToken } from './token-api'
import {
  buildTokenFormSchema,
  defaultTokenValues,
  toTokenRequest,
  type TokenEditorMode,
  type TokenFormValues,
} from './token-form-model'

type TokenFormProps = {
  mode: TokenEditorMode
  token?: AdminToken
  onCancel: () => void
  onIssued: (issued: IssuedAdminToken) => void
  onSaved: () => void
}

type FieldProps = {
  id: string
  label: string
  error?: string
  hint?: string
  children: ReactNode
}

function Field({ id, label, error, hint, children }: FieldProps) {
  return (
    <div className="grid gap-1.5">
      <Label htmlFor={id}>{label}</Label>
      {children}
      {error ? <p className="text-xs text-destructive">{error}</p> : null}
      {!error && hint ? <p className="text-[0.6875rem] leading-4 text-muted-foreground">{hint}</p> : null}
    </div>
  )
}

function SwitchField({ id, label, hint, checked, onCheckedChange }: {
  id: string
  label: string
  hint: string
  checked: boolean
  onCheckedChange: (checked: boolean) => void
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
      <div><Label htmlFor={id}>{label}</Label><p className="mt-1 text-[0.6875rem] text-muted-foreground">{hint}</p></div>
      <Switch id={id} checked={checked} onCheckedChange={onCheckedChange} />
    </div>
  )
}

export function TokenForm({ mode, token, onCancel, onIssued, onSaved }: TokenFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateAdminToken()
  const updateMutation = useUpdateAdminToken()
  const schema = buildTokenFormSchema({
    invalidField: t('tokens.validation.field'),
    invalidModels: t('tokens.validation.models'),
    invalidIps: t('tokens.validation.ips'),
    invalidDate: t('tokens.validation.date'),
  })
  const form = useForm<TokenFormValues>({
    defaultValues: defaultTokenValues(token),
    resolver: zodResolver(schema),
  })

  useEffect(() => {
    form.reset(defaultTokenValues(token))
  }, [form, mode, token])

  const unlimitedQuota = form.watch('unlimitedQuota')
  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.isError || updateMutation.isError
  const onSubmit = form.handleSubmit(async (values) => {
    try {
      if (mode === 'create') {
        const issued = await createMutation.mutateAsync(toTokenRequest(values))
        onIssued(issued)
      } else if (token) {
        await updateMutation.mutateAsync({ id: token.id, body: toTokenRequest(values) })
        onSaved()
      }
    } catch {
      // 保留用户输入和一次性签发边界，服务端错误由 mutation 状态统一呈现。
    }
  })

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={onSubmit} noValidate>
      <div className="flex-1 space-y-5 overflow-y-auto px-4 pb-5">
        <section className="grid gap-3" aria-labelledby="token-basic-fields">
          <h3 id="token-basic-fields" className="text-xs font-semibold">{t('tokens.form.basic')}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field id="token-name" label={t('tokens.fields.name')} error={form.formState.errors.name?.message}>
              <Input id="token-name" aria-invalid={!!form.formState.errors.name} {...form.register('name')} />
            </Field>
            <Field id="token-user" label={t('tokens.fields.userId')} error={form.formState.errors.userId?.message} hint={mode === 'update' ? t('tokens.form.ownerHint') : undefined}>
              <Input id="token-user" type="number" min={1} readOnly={mode === 'update'} aria-invalid={!!form.formState.errors.userId} {...form.register('userId', { valueAsNumber: true })} />
            </Field>
          </div>
          <div className="grid gap-3 sm:grid-cols-3">
            <Field id="token-status" label={t('tokens.fields.status')}>
              <Select id="token-status" {...form.register('status')}><option value="enabled">{t('tokens.status.enabled')}</option><option value="disabled">{t('tokens.status.disabled')}</option></Select>
            </Field>
            <Field id="token-group" label={t('tokens.fields.groupId')} error={form.formState.errors.groupId?.message} hint={t('tokens.form.groupHint')}>
              <Input id="token-group" type="number" min={1} aria-invalid={!!form.formState.errors.groupId} {...form.register('groupId')} />
            </Field>
            <Field id="token-expiry" label={t('tokens.fields.expiredAt')} error={form.formState.errors.expiredAt?.message} hint={t('tokens.form.expiryHint')}>
              <Input id="token-expiry" type="datetime-local" aria-invalid={!!form.formState.errors.expiredAt} {...form.register('expiredAt')} />
            </Field>
          </div>
        </section>

        <section className="grid gap-3" aria-labelledby="token-quota-fields">
          <h3 id="token-quota-fields" className="text-xs font-semibold">{t('tokens.form.quota')}</h3>
          <Controller control={form.control} name="unlimitedQuota" render={({ field }) => <SwitchField id="token-unlimited" label={t('tokens.fields.unlimitedQuota')} hint={t('tokens.form.unlimitedHint')} checked={field.value} onCheckedChange={field.onChange} />} />
          <div className="grid gap-3 sm:grid-cols-2">
            <Field id="token-quota" label={t('tokens.fields.remainQuota')} error={form.formState.errors.remainQuota?.message}>
              <Input id="token-quota" type="number" min={0} disabled={unlimitedQuota} aria-invalid={!!form.formState.errors.remainQuota} {...form.register('remainQuota', { valueAsNumber: true })} />
            </Field>
            <Field id="token-max-requests" label={t('tokens.fields.maxRequests')} error={form.formState.errors.maxRequests?.message} hint={t('tokens.form.optionalLimit')}>
              <Input id="token-max-requests" type="number" min={0} aria-invalid={!!form.formState.errors.maxRequests} {...form.register('maxRequests')} />
            </Field>
          </div>
          <Controller control={form.control} name="crossGroupRetry" render={({ field }) => <SwitchField id="token-cross-group" label={t('tokens.fields.crossGroupRetry')} hint={t('tokens.form.crossGroupHint')} checked={field.value} onCheckedChange={field.onChange} />} />
        </section>

        <section className="grid gap-3" aria-labelledby="token-policy-fields">
          <h3 id="token-policy-fields" className="text-xs font-semibold">{t('tokens.form.policy')}</h3>
          <Field id="token-models" label={t('tokens.fields.modelLimits')} error={form.formState.errors.modelLimits?.message} hint={t('tokens.form.modelsHint')}>
            <Textarea id="token-models" className="min-h-24 font-mono text-xs" aria-invalid={!!form.formState.errors.modelLimits} {...form.register('modelLimits')} />
          </Field>
          <Field id="token-ips" label={t('tokens.fields.allowIps')} error={form.formState.errors.allowIps?.message} hint={t('tokens.form.ipsHint')}>
            <Textarea id="token-ips" className="min-h-24 font-mono text-xs" aria-invalid={!!form.formState.errors.allowIps} {...form.register('allowIps')} />
          </Field>
        </section>

        <section className="grid gap-3" aria-labelledby="token-rate-fields">
          <h3 id="token-rate-fields" className="text-xs font-semibold">{t('tokens.form.rateLimits')}</h3>
          <div className="grid gap-3 sm:grid-cols-3">
            {(['rateLimit5h', 'rateLimit1d', 'rateLimit7d'] as const).map((field) => (
              <Field key={field} id={`token-${field}`} label={t(`tokens.fields.${field}`)} error={form.formState.errors[field]?.message} hint={t('tokens.form.optionalLimit')}>
                <Input id={`token-${field}`} type="number" min={0} aria-invalid={!!form.formState.errors[field]} {...form.register(field)} />
              </Field>
            ))}
          </div>
        </section>
        {submitError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('tokens.form.submitError')}</p> : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="secondary" onClick={onCancel}>{t('tokens.actions.cancel')}</Button>
        <Button type="submit" disabled={pending}>{pending ? <LoaderCircle className="animate-spin" /> : null}{t(mode === 'create' ? 'tokens.actions.create' : 'tokens.actions.save')}</Button>
      </div>
    </form>
  )
}
