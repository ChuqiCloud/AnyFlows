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
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { useCreateAdminChannel, useUpdateAdminChannel } from './channel-api'
import {
  buildChannelFormSchema,
  defaultChannelValues,
  toCreateRequest,
  toUpdateRequest,
  type ChannelEditorMode,
  type ChannelFormValues,
} from './channel-form-model'

type ChannelFormProps = {
  mode: ChannelEditorMode
  channel?: AdminChannel
  onCancel: () => void
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

export function ChannelForm({ mode, channel, onCancel, onSaved }: ChannelFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateAdminChannel()
  const updateMutation = useUpdateAdminChannel()
  const schema = buildChannelFormSchema(mode, {
    invalidField: t('channels.validation.field'),
    invalidJson: t('channels.validation.json'),
    invalidStringMap: t('channels.validation.stringMap'),
    invalidUrl: t('channels.validation.url'),
    invalidRouting: t('channels.validation.routing'),
    sensitiveRequired: t('channels.validation.sensitiveRequired'),
  })
  const form = useForm<ChannelFormValues>({
    defaultValues: defaultChannelValues(channel),
    resolver: zodResolver(schema),
  })

  useEffect(() => {
    form.reset(defaultChannelValues(channel))
  }, [channel, form, mode])

  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.isError || updateMutation.isError
  const onSubmit = form.handleSubmit(async (values) => {
    try {
      if (mode === 'create') {
        await createMutation.mutateAsync(toCreateRequest(values))
      } else if (channel) {
        await updateMutation.mutateAsync({ id: channel.id, body: toUpdateRequest(values) })
      }
      onSaved()
    } catch {
      // Mutation 状态负责呈现服务端错误，表单内容保持不变便于修正后重试。
    }
  })

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={onSubmit} noValidate>
      <div className="flex-1 space-y-5 overflow-y-auto px-4 pb-5">
        <section className="grid gap-3" aria-labelledby="channel-basic-fields">
          <h3 id="channel-basic-fields" className="text-xs font-semibold">{t('channels.form.basic')}</h3>
          <Field id="channel-name" label={t('channels.fields.name')} error={form.formState.errors.name?.message}>
            <Input id="channel-name" aria-invalid={!!form.formState.errors.name} {...form.register('name')} />
          </Field>
          <Field id="channel-url" label={t('channels.fields.baseUrl')} error={form.formState.errors.baseUrl?.message}>
            <Input id="channel-url" placeholder="https://api.example.com/v1" aria-invalid={!!form.formState.errors.baseUrl} {...form.register('baseUrl')} />
          </Field>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field id="channel-type" label={t('channels.fields.type')}><Input id="channel-type" value="OpenAI" readOnly /></Field>
            <Field id="channel-protocol" label={t('channels.fields.protocol')}><Input id="channel-protocol" value="OpenAI Chat" readOnly /></Field>
          </div>
          <div className="grid gap-3 sm:grid-cols-3">
            <Field id="channel-status" label={t('channels.fields.status')}>
              <Select id="channel-status" {...form.register('status')}>
                <option value="enabled">{t('channels.status.enabled')}</option>
                <option value="disabled">{t('channels.status.disabled')}</option>
              </Select>
            </Field>
            <Field id="channel-weight" label={t('channels.fields.weight')} error={form.formState.errors.weight?.message}>
              <Input id="channel-weight" type="number" min={0} aria-invalid={!!form.formState.errors.weight} {...form.register('weight', { valueAsNumber: true })} />
            </Field>
            <Field id="channel-priority" label={t('channels.fields.priority')} error={form.formState.errors.priority?.message}>
              <Input id="channel-priority" type="number" aria-invalid={!!form.formState.errors.priority} {...form.register('priority', { valueAsNumber: true })} />
            </Field>
          </div>
          <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
            <div><Label htmlFor="channel-auto-ban">{t('channels.fields.autoBan')}</Label><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channels.form.autoBanHint')}</p></div>
            <Controller control={form.control} name="autoBan" render={({ field }) => <Switch id="channel-auto-ban" checked={field.value} onCheckedChange={field.onChange} />} />
          </div>
          <Field id="channel-tag" label={t('channels.fields.tag')}><Input id="channel-tag" {...form.register('tag')} /></Field>
        </section>

        <section className="grid gap-3" aria-labelledby="channel-routing-fields">
          <h3 id="channel-routing-fields" className="text-xs font-semibold">{t('channels.form.routing')}</h3>
          <Field id="channel-models" label={t('channels.fields.models')} hint={t('channels.form.modelsHint')}>
            <Textarea id="channel-models" className="font-mono text-xs" {...form.register('models')} />
          </Field>
          <Field id="channel-groups" label={t('channels.fields.groupIds')} error={form.formState.errors.groupIds?.message} hint={t('channels.form.groupsHint')}>
            <Input id="channel-groups" className="font-mono" aria-invalid={!!form.formState.errors.groupIds} {...form.register('groupIds')} />
          </Field>
        </section>

        <section className="grid gap-3" aria-labelledby="channel-json-fields">
          <h3 id="channel-json-fields" className="text-xs font-semibold">{t('channels.form.advanced')}</h3>
          <Field id="channel-mapping" label={t('channels.fields.modelMapping')} error={form.formState.errors.modelMapping?.message}>
            <Textarea id="channel-mapping" className="min-h-24 font-mono text-xs" aria-invalid={!!form.formState.errors.modelMapping} {...form.register('modelMapping')} />
          </Field>
          <Field id="channel-params" label={t('channels.fields.paramOverride')} error={form.formState.errors.paramOverride?.message}>
            <Textarea id="channel-params" className="min-h-24 font-mono text-xs" aria-invalid={!!form.formState.errors.paramOverride} {...form.register('paramOverride')} />
          </Field>
          <Field id="channel-headers" label={t('channels.fields.headerOverride')} error={form.formState.errors.headerOverride?.message} hint={mode === 'update' ? t('channels.form.sensitiveHint') : undefined}>
            <Textarea id="channel-headers" className="min-h-24 font-mono text-xs" aria-invalid={!!form.formState.errors.headerOverride} {...form.register('headerOverride')} />
          </Field>
          <Field id="channel-settings" label={t('channels.fields.settings')} error={form.formState.errors.settings?.message} hint={mode === 'update' ? t('channels.form.sensitiveHint') : undefined}>
            <Textarea id="channel-settings" className="min-h-24 font-mono text-xs" aria-invalid={!!form.formState.errors.settings} {...form.register('settings')} />
          </Field>
        </section>
        {submitError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('channels.form.submitError')}</p> : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="secondary" onClick={onCancel}>{t('channels.actions.cancel')}</Button>
        <Button type="submit" disabled={pending}>{pending ? <LoaderCircle className="animate-spin" /> : null}{t(mode === 'create' ? 'channels.actions.create' : 'channels.actions.save')}</Button>
      </div>
    </form>
  )
}
