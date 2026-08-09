import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { LoaderCircle } from 'lucide-react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Switch } from '@/components/ui/switch'
import type { AdminChannel } from '@/lib/api/generated/types.gen'
import { useCreateAdminChannel, useUpdateAdminChannel } from './channel-api'
import { ChannelAutoBanRuleEditor } from './channel-auto-ban-rule-editor'
import { ChannelFormField } from './channel-form-field'
import {
  buildChannelFormSchema,
  createKeyValueRow,
  defaultChannelValues,
  MAX_CHANNEL_TIMEOUT_SECONDS,
  toCreateRequest,
  toUpdateRequest,
  type ChannelEditorMode,
  type ChannelFormValues,
  type ChannelProtocol,
  type ChannelType,
} from './channel-form-model'
import { ChannelGroupSelector } from './channel-group-selector'
import { ChannelKeyValueEditor } from './channel-key-value-editor'
import { ChannelModelEditor } from './channel-model-editor'
import { ChannelParameterEditor } from './channel-parameter-editor'
import { ChannelHeaderOverrideField } from './channel-sensitive-fields'

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
    invalidParameter: t('channels.validation.parameter'),
    invalidAutoBanRules: t('channels.validation.autoBanRules'),
    invalidRouting: t('channels.validation.routing'),
    invalidTimeout: t('channels.validation.timeout'),
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
  const channelType = form.watch('channelType')
  const protocol = form.watch('protocol')
  const autoBan = form.watch('autoBan')
  const poolMode = form.watch('poolMode')
  const removeUnsupportedParameters = (nextProtocol: ChannelProtocol) => {
    if (nextProtocol === 'openai_embeddings' || nextProtocol === 'openai_images') {
      form.setValue('paramOverride', [], { shouldDirty: true, shouldValidate: true })
      return
    }
    if (nextProtocol !== 'openai_responses') return
    form.setValue(
      'paramOverride',
      form.getValues('paramOverride').filter((row) => row.key !== 'stop_sequences'),
      { shouldDirty: true, shouldValidate: true },
    )
  }
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
          <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(11rem,0.36fr)]">
            <ChannelFormField id="channel-url" label={t('channels.fields.baseUrl')} error={form.formState.errors.baseUrl?.message}>
              <Input id="channel-url" placeholder="https://api.example.com/v1" aria-invalid={!!form.formState.errors.baseUrl} {...form.register('baseUrl')} />
            </ChannelFormField>
            <ChannelFormField id="channel-timeout" label={t('channels.fields.timeout')} error={form.formState.errors.timeoutSeconds?.message} hint={t('channels.form.timeoutHint')}>
              <Input
                id="channel-timeout"
                type="number"
                min={1}
                max={MAX_CHANNEL_TIMEOUT_SECONDS}
                step={1}
                placeholder={t('channels.form.timeoutPlaceholder')}
                aria-invalid={!!form.formState.errors.timeoutSeconds}
                {...form.register('timeoutSeconds')}
              />
            </ChannelFormField>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <ChannelFormField id="channel-type" label={t('channels.fields.type')}>
              <Controller control={form.control} name="channelType" render={({ field }) => (
                <Select
                  id="channel-type"
                  value={field.value}
                  onBlur={field.onBlur}
                  name={field.name}
                  ref={field.ref}
                  onChange={(event) => {
                    const nextType = event.target.value as ChannelType
                    const currentProtocol = form.getValues('protocol')
                    const nextProtocol = nextType === 'anthropic'
                      ? 'anthropic'
                      : nextType === 'gemini'
                        ? 'gemini'
                        : currentProtocol === 'openai_chat'
                          || currentProtocol === 'openai_responses'
                          || currentProtocol === 'openai_embeddings'
                          || currentProtocol === 'openai_images'
                          ? currentProtocol
                          : 'openai_chat'
                    field.onChange(nextType)
                    form.setValue('protocol', nextProtocol, {
                      shouldDirty: true,
                      shouldValidate: true,
                    })
                    if (nextType !== 'openai' || nextProtocol !== 'openai_responses') {
                      form.setValue('responsesWebsocketEnabled', false, { shouldDirty: true })
                    }
                    removeUnsupportedParameters(nextProtocol)
                  }}
                >
                  <option value="openai">{t('channels.type.openai')}</option>
                  <option value="anthropic">{t('channels.type.anthropic')}</option>
                  <option value="gemini">{t('channels.type.gemini')}</option>
                </Select>
              )} />
            </ChannelFormField>
            <ChannelFormField id="channel-protocol" label={t('channels.fields.protocol')}>
              <Controller control={form.control} name="protocol" render={({ field }) => (
                <Select
                  id="channel-protocol"
                  value={field.value}
                  onBlur={field.onBlur}
                  name={field.name}
                  ref={field.ref}
                  onChange={(event) => {
                    const nextProtocol = event.target.value as ChannelFormValues['protocol']
                    field.onChange(nextProtocol)
                    removeUnsupportedParameters(nextProtocol)
                    if (nextProtocol !== 'openai_responses') {
                      form.setValue('responsesWebsocketEnabled', false, { shouldDirty: true })
                    }
                  }}
                >
                  {channelType === 'anthropic' ? (
                    <option value="anthropic">{t('channels.protocol.anthropic')}</option>
                  ) : channelType === 'gemini' ? (
                    <option value="gemini">{t('channels.protocol.gemini')}</option>
                  ) : (
                    <>
                      <option value="openai_chat">{t('channels.protocol.openai_chat')}</option>
                      <option value="openai_responses">{t('channels.protocol.openai_responses')}</option>
                      <option value="openai_embeddings">{t('channels.protocol.openai_embeddings')}</option>
                      <option value="openai_images">{t('channels.protocol.openai_images')}</option>
                    </>
                  )}
                </Select>
              )} />
            </ChannelFormField>
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
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
              <div><Label htmlFor="channel-pool-mode">{t('channels.fields.poolMode')}</Label><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channels.form.poolModeHint')}</p></div>
              <Controller control={form.control} name="poolMode" render={({ field }) => <Switch id="channel-pool-mode" checked={field.value} onCheckedChange={field.onChange} />} />
            </div>
            <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
              <div><Label htmlFor="channel-auto-ban">{t('channels.fields.autoBan')}</Label><p className="mt-1 text-[0.6875rem] text-muted-foreground">{t(poolMode ? 'channels.form.autoBanPoolModeHint' : 'channels.form.autoBanHint')}</p></div>
              <Controller control={form.control} name="autoBan" render={({ field }) => <Switch id="channel-auto-ban" checked={field.value} disabled={poolMode} onCheckedChange={field.onChange} />} />
            </div>
          </div>
          {autoBan && !poolMode ? (
            <ChannelFormField
              id="channel-auto-ban-rules"
              htmlFor={false}
              label={t('channels.fields.autoBanRules')}
              hint={t('channels.form.autoBanRulesHint')}
              error={form.formState.errors.autoBanKeywords?.message || form.formState.errors.autoBanStatusCodes?.message}
            >
              <ChannelAutoBanRuleEditor
                statusCodes={form.watch('autoBanStatusCodes')}
                keywords={form.watch('autoBanKeywords')}
                invalid={!!form.formState.errors.autoBanKeywords || !!form.formState.errors.autoBanStatusCodes}
                onStatusCodesChange={(statusCodes) => form.setValue('autoBanStatusCodes', statusCodes, {
                  shouldDirty: true,
                  shouldValidate: true,
                })}
                onKeywordsChange={(keywords) => form.setValue('autoBanKeywords', keywords, {
                  shouldDirty: true,
                  shouldValidate: true,
                })}
              />
            </ChannelFormField>
          ) : null}
          {channelType === 'openai' && protocol === 'openai_responses' ? (
            <div className="flex items-center justify-between gap-4 rounded-lg border border-[var(--hairline)] px-3 py-2.5">
              <div>
                <Label htmlFor="channel-responses-websocket">{t('channels.fields.responsesWebsocket')}</Label>
                <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channels.form.responsesWebsocketHint')}</p>
              </div>
              <Controller control={form.control} name="responsesWebsocketEnabled" render={({ field }) => (
                <Switch id="channel-responses-websocket" checked={field.value} onCheckedChange={field.onChange} />
              )} />
            </div>
          ) : null}
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
          {protocol !== 'openai_embeddings' && protocol !== 'openai_images' ? (
            <ChannelFormField id="channel-params" htmlFor={false} label={t('channels.fields.paramOverride')} error={form.formState.errors.paramOverride?.message} hint={t('channels.form.parametersHint')}>
              <Controller control={form.control} name="paramOverride" render={({ field }) => <ChannelParameterEditor protocol={protocol} rows={field.value} onChange={field.onChange} />} />
            </ChannelFormField>
          ) : null}
          <ChannelHeaderOverrideField mode={mode} form={form} />
        </section>

        {submitError ? <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">{t('channels.form.submitError')}</p> : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="secondary" onClick={onCancel}>{t('channels.actions.cancel')}</Button>
        <Button type="submit" disabled={pending}>{pending ? <LoaderCircle className="animate-spin" aria-hidden="true" /> : null}{t(mode === 'create' ? 'channels.actions.create' : 'channels.actions.save')}</Button>
      </div>
    </form>
  )
}
