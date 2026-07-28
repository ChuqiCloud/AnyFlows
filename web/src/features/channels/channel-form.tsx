import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { LoaderCircle } from 'lucide-react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { useCreateAdminChannel, useUpdateAdminChannel } from './channel-api'
import { ChannelFormField } from './channel-form-field'
import {
  buildChannelFormSchema,
  createKeyValueRow,
  defaultChannelValues,
  toCreateRequest,
  toUpdateRequest,
  type ChannelEditorMode,
  type ChannelFormValues,
} from './channel-form-model'
import { ChannelGroupSelector } from './channel-group-selector'
import { ChannelKeyValueEditor } from './channel-key-value-editor'
import { ChannelModelEditor } from './channel-model-editor'
import { ChannelParameterEditor } from './channel-parameter-editor'
import { ChannelAdvancedSettings, ChannelHeaderOverrideField } from './channel-sensitive-fields'

type ChannelFormProps = {
  mode: ChannelEditorMode
  channel?: AdminChannel
  onCancel: () => void
  onSaved: (channel: AdminChannel) => void
}

export function ChannelForm({ mode, channel, onCancel, onSaved }: ChannelFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateAdminChannel()
  const updateMutation = useUpdateAdminChannel()
  const schema = buildChannelFormSchema(mode, {
    duplicateKey: t('channels.validation.duplicateKey'),
    invalidEntries: t('channels.validation.entries'),
    invalidField: t('channels.validation.field'),
    invalidHeader: t('channels.validation.header'),
    invalidJson: t('channels.validation.json'),
    invalidParameter: t('channels.validation.parameter'),
    invalidRouting: t('channels.validation.routing'),
    invalidUrl: t('channels.validation.url'),
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
      const savedChannel = mode === 'create'
        ? await createMutation.mutateAsync(toCreateRequest(values))
        : channel
          ? await updateMutation.mutateAsync({ id: channel.id, body: toUpdateRequest(values) })
          : undefined
      if (savedChannel) onSaved(savedChannel)
    } catch {
      // Mutation 状态负责呈现服务端错误，保留表单内容便于修正后重试。
    }
  })

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={onSubmit} noValidate>
      <div className="flex-1 space-y-6 overflow-y-auto px-4 py-5">
        <section className="grid gap-3" aria-labelledby="channel-basic-fields">
          <h3 id="channel-basic-fields" className="text-xs font-semibold">{t('channels.form.basic')}</h3>
          <ChannelFormField id="channel-name" label={t('channels.fields.name')} error={form.formState.errors.name?.message}>
            <Input id="channel-name" aria-invalid={!!form.formState.errors.name} {...form.register('name')} />
          </ChannelFormField>
          <ChannelFormField id="channel-url" label={t('channels.fields.baseUrl')} error={form.formState.errors.baseUrl?.message}>
            <Input id="channel-url" placeholder="https://api.example.com/v1" aria-invalid={!!form.formState.errors.baseUrl} {...form.register('baseUrl')} />
          </ChannelFormField>
          <div className="grid gap-3 sm:grid-cols-2">
            <StaticBadgeField label={t('channels.fields.type')} value="OpenAI" />
            <StaticBadgeField label={t('channels.fields.protocol')} value="OpenAI Chat" />
          </div>
          <div className="grid gap-3 sm:grid-cols-3">
            <ChannelFormField id="channel-status" label={t('channels.fields.status')}>
              <Select id="channel-status" {...form.register('status')}>
                <option value="enabled">{t('channels.status.enabled')}</option>
                <option value="disabled">{t('channels.status.disabled')}</option>
              </Select>
            </ChannelFormField>
            <ChannelFormField id="channel-weight" label={t('channels.fields.weight')} error={form.formState.errors.weight?.message}>
              <Input id="channel-weight" type="number" min={0} step={1} aria-invalid={!!form.formState.errors.weight} {...form.register('weight', { valueAsNumber: true })} />
            </ChannelFormField>
            <ChannelFormField id="channel-priority" label={t('channels.fields.priority')} error={form.formState.errors.priority?.message}>
              <Input id="channel-priority" type="number" step={1} aria-invalid={!!form.formState.errors.priority} {...form.register('priority', { valueAsNumber: true })} />
            </ChannelFormField>
          </div>
          <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
            <div><Label htmlFor="channel-auto-ban">{t('channels.fields.autoBan')}</Label><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channels.form.autoBanHint')}</p></div>
            <Controller control={form.control} name="autoBan" render={({ field }) => <Switch id="channel-auto-ban" checked={field.value} onCheckedChange={field.onChange} />} />
          </div>
          <ChannelFormField id="channel-tag" label={t('channels.fields.tag')} hint={t('channels.form.tagHint')}>
            <Input id="channel-tag" {...form.register('tag')} />
          </ChannelFormField>
        </section>

        <section className="grid gap-3" aria-labelledby="channel-routing-fields">
          <h3 id="channel-routing-fields" className="text-xs font-semibold">{t('channels.form.routing')}</h3>
          <ChannelFormField id="channel-models" label={t('channels.fields.models')} error={form.formState.errors.models?.message} hint={t('channels.form.modelsHint')}>
            <Controller control={form.control} name="models" render={({ field }) => <ChannelModelEditor id="channel-models" value={field.value} invalid={!!form.formState.errors.models} onChange={field.onChange} />} />
          </ChannelFormField>
          <ChannelFormField id="channel-groups" label={t('channels.fields.groupIds')} error={form.formState.errors.groupIds?.message} hint={t('channels.form.groupsHint')}>
            <Controller control={form.control} name="groupIds" render={({ field }) => <ChannelGroupSelector id="channel-groups" value={field.value} invalid={!!form.formState.errors.groupIds} onChange={field.onChange} />} />
          </ChannelFormField>
        </section>

        <section className="grid gap-4" aria-labelledby="channel-override-fields">
          <h3 id="channel-override-fields" className="text-xs font-semibold">{t('channels.form.overrides')}</h3>
          <ChannelFormField id="channel-mapping" htmlFor={false} label={t('channels.fields.modelMapping')} error={form.formState.errors.modelMapping?.message} hint={t('channels.form.mappingHint')}>
            <Controller control={form.control} name="modelMapping" render={({ field }) => (
              <ChannelKeyValueEditor rows={field.value} keyLabel={t('channels.mapping.source')} valueLabel={t('channels.mapping.target')} addLabel={t('channels.mapping.add')} removeLabel={t('channels.mapping.remove')} emptyText={t('channels.mapping.empty')} createRow={() => createKeyValueRow('mapping')} onChange={field.onChange} />
            )} />
          </ChannelFormField>
          <ChannelFormField id="channel-params" htmlFor={false} label={t('channels.fields.paramOverride')} error={form.formState.errors.paramOverride?.message} hint={t('channels.form.parametersHint')}>
            <Controller control={form.control} name="paramOverride" render={({ field }) => <ChannelParameterEditor rows={field.value} onChange={field.onChange} />} />
          </ChannelFormField>
          <ChannelHeaderOverrideField mode={mode} form={form} />
        </section>

        <ChannelAdvancedSettings mode={mode} form={form} />

        {submitError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('channels.form.submitError')}</p> : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="secondary" onClick={onCancel}>{t('channels.actions.cancel')}</Button>
        <Button type="submit" disabled={pending}>{pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t(mode === 'create' ? 'channels.actions.create' : 'channels.actions.save')}</Button>
      </div>
    </form>
  )
}

function StaticBadgeField({ label, value }: { label: string; value: string }) {
  return <div className="grid gap-1.5"><span className="text-xs font-medium">{label}</span><div className="flex h-8 items-center"><Badge className="h-6 border-transparent bg-info/10 px-2 text-info">{value}</Badge></div></div>
}
