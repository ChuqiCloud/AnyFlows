import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { LoaderCircle, ShieldAlert } from 'lucide-react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { ProviderPicker } from '@/components/brand/provider-picker'
import { findProvider } from '@/components/brand/provider-catalog'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
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
  type ClientSimulationBodyProfile,
  type ClientSimulationProfile,
  type ResponsesCompactMode,
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

const adapterOptions = ['openai', 'anthropic', 'gemini', 'jina', 'cohere', 'xai']
  .map((id) => ({ ...findProvider(id)!, id }))
const channelTypeOptions = [
  findProvider('codex_oauth')!,
  ...adapterOptions,
]

export function ChannelForm({ mode, channel, onCancel, onSaved }: ChannelFormProps) {
  const { t, i18n } = useTranslation()
  const createMutation = useCreateAdminChannel()
  const updateMutation = useUpdateAdminChannel()
  const existingClientSimulationProfile = channel?.client_simulation_profile ?? ''
  const existingClientSimulationBodyProfile = channel?.client_simulation_body_profile ?? ''
  const schema = buildChannelFormSchema(mode, {
    duplicateKey: t('channels.validation.duplicateKey'),
    invalidEntries: t('channels.validation.entries'),
    invalidField: t('channels.validation.field'),
    invalidHeader: t('channels.validation.header'),
    invalidParameter: t('channels.validation.parameter'),
    invalidAutoBanRules: t('channels.validation.autoBanRules'),
    invalidClientSimulationRisk: t('channels.validation.clientSimulationRisk'),
    invalidClientSimulationBodyRisk: t('channels.validation.clientSimulationBodyRisk'),
    invalidRouting: t('channels.validation.routing'),
    invalidTimeout: t('channels.validation.timeout'),
    invalidUrl: t('channels.validation.url'),
  }, existingClientSimulationProfile, existingClientSimulationBodyProfile)
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
  const codexOAuth = channelType === 'codex_oauth'
  const autoBan = form.watch('autoBan')
  const poolMode = form.watch('poolMode')
  const clientSimulationProfile = form.watch('clientSimulationProfile')
  const clientSimulationBodyProfile = form.watch('clientSimulationBodyProfile')
  const clientSimulationRiskRequired = clientSimulationProfile !== ''
    && clientSimulationProfile !== existingClientSimulationProfile
  const clientSimulationBodyRiskRequired = clientSimulationBodyProfile !== ''
    && clientSimulationBodyProfile !== existingClientSimulationBodyProfile
  const removeUnsupportedParameters = (nextProtocol: ChannelProtocol) => {
    if (nextProtocol === 'openai_embeddings' || nextProtocol === 'openai_images' || nextProtocol === 'openai_audio' || nextProtocol === 'openai_speech' || nextProtocol === 'jina_rerank' || nextProtocol === 'cohere_rerank' || nextProtocol === 'xai_video') {
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
  const resetUnsupportedResponsesOptions = (
    nextType: ChannelType,
    nextProtocol: ChannelProtocol,
  ) => {
    if ((nextType === 'openai' || nextType === 'codex_oauth') && nextProtocol === 'openai_responses') return
    form.setValue('responsesWebsocketEnabled', false, { shouldDirty: true })
    form.setValue('responsesCompactMode', 'auto', { shouldDirty: true })
    form.setValue('responsesCompactModelMapping', [], { shouldDirty: true })
  }
  const resetUnsupportedClientSimulation = (
    nextType: ChannelType,
    nextProtocol: ChannelProtocol,
  ) => {
    if (nextType === 'anthropic' && nextProtocol === 'anthropic') return
    form.setValue('clientSimulationProfile', '', { shouldDirty: true, shouldValidate: true })
    form.setValue('clientSimulationRiskAccepted', false, {
      shouldDirty: true,
      shouldValidate: true,
    })
    form.setValue('clientSimulationBodyProfile', '', { shouldDirty: true, shouldValidate: true })
    form.setValue('clientSimulationBodyRiskAccepted', false, {
      shouldDirty: true,
      shouldValidate: true,
    })
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
            {codexOAuth ? (
              <ChannelFormField id="channel-url" label={t('channels.fields.baseUrl')} hint={t('channels.form.codexOAuthBaseUrlHint')}>
                <div className="flex h-9 items-center rounded-lg border border-[var(--hairline)] bg-surface-2 px-3 text-sm text-muted-foreground">{t('channels.form.codexOAuthFixedEndpoint')}</div>
              </ChannelFormField>
            ) : (
              <ChannelFormField id="channel-url" label={t('channels.fields.baseUrl')} error={form.formState.errors.baseUrl?.message}>
                <Input id="channel-url" placeholder="https://api.example.com/v1" aria-invalid={!!form.formState.errors.baseUrl} {...form.register('baseUrl')} />
              </ChannelFormField>
            )}
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
            <ChannelFormField id="channel-provider" label={t('providers.label')} error={form.formState.errors.provider?.message}>
              <Controller control={form.control} name="provider" render={({ field }) => <ProviderPicker id="channel-provider" {...field} invalid={!!form.formState.errors.provider} disabled={pending || codexOAuth} />} />
            </ChannelFormField>
            <ChannelFormField id="channel-type" label={t('channels.fields.type')} hint={t('providers.adapterHint')}>
              <Controller control={form.control} name="channelType" render={({ field }) => (
                <ProviderPicker
                  id="channel-type"
                  value={field.value}
                  onBlur={field.onBlur}
                  ref={field.ref}
                  allowCustom={false}
                  options={channelTypeOptions}
                  onChange={(value) => {
                    const nextType = value as ChannelType
                    const currentProtocol = form.getValues('protocol')
                    const nextProtocol = nextType === 'codex_oauth'
                      ? 'openai_responses'
                      : nextType === 'anthropic'
                      ? 'anthropic'
                      : nextType === 'gemini'
                        ? 'gemini'
                        : nextType === 'jina'
                          ? 'jina_rerank'
                          : nextType === 'cohere'
                            ? 'cohere_rerank'
                            : nextType === 'xai'
                              ? 'xai_video'
                          : currentProtocol === 'openai_chat'
                          || currentProtocol === 'openai_responses'
                          || currentProtocol === 'openai_embeddings'
                          || currentProtocol === 'openai_images'
                          || currentProtocol === 'openai_audio'
                          || currentProtocol === 'openai_speech'
                          ? currentProtocol
                          : 'openai_chat'
                    field.onChange(nextType)
                    if (nextType === 'codex_oauth') {
                      form.setValue('provider', 'codex', { shouldDirty: true, shouldValidate: true })
                      form.setValue('baseUrl', '', { shouldDirty: true, shouldValidate: true })
                      form.setValue('responsesWebsocketEnabled', false, { shouldDirty: true, shouldValidate: true })
                    } else if (form.getValues('provider') === 'codex') {
                      form.setValue('provider', nextType, { shouldDirty: true, shouldValidate: true })
                    }
                    form.setValue('protocol', nextProtocol, {
                      shouldDirty: true,
                      shouldValidate: true,
                    })
                    resetUnsupportedResponsesOptions(nextType, nextProtocol)
                    resetUnsupportedClientSimulation(nextType, nextProtocol)
                    removeUnsupportedParameters(nextProtocol)
                  }}
                />
              )} />
            </ChannelFormField>
            <ChannelFormField id="channel-protocol" label={t('channels.fields.protocol')}>
              <Controller control={form.control} name="protocol" render={({ field }) => (
                <Select
                  id="channel-protocol"
                  value={field.value}
                  disabled={codexOAuth}
                  onBlur={field.onBlur}
                  name={field.name}
                  ref={field.ref}
                  onChange={(event) => {
                    const nextProtocol = event.target.value as ChannelFormValues['protocol']
                    field.onChange(nextProtocol)
                    removeUnsupportedParameters(nextProtocol)
                    resetUnsupportedResponsesOptions(channelType, nextProtocol)
                    resetUnsupportedClientSimulation(channelType, nextProtocol)
                  }}
                >
                  {codexOAuth ? (
                    <option value="openai_responses">{t('channels.protocol.openai_responses')}</option>
                  ) : channelType === 'anthropic' ? (
                    <option value="anthropic">{t('channels.protocol.anthropic')}</option>
                  ) : channelType === 'gemini' ? (
                    <option value="gemini">{t('channels.protocol.gemini')}</option>
                  ) : channelType === 'jina' ? (
                    <option value="jina_rerank">{t('channels.protocol.jina_rerank')}</option>
                  ) : channelType === 'cohere' ? (
                    <option value="cohere_rerank">{t('channels.protocol.cohere_rerank')}</option>
                  ) : channelType === 'xai' ? (
                    <option value="xai_video">{t('channels.protocol.xai_video')}</option>
                  ) : (
                    <>
                      <option value="openai_chat">{t('channels.protocol.openai_chat')}</option>
                      <option value="openai_responses">{t('channels.protocol.openai_responses')}</option>
                      <option value="openai_embeddings">{t('channels.protocol.openai_embeddings')}</option>
                      <option value="openai_images">{t('channels.protocol.openai_images')}</option>
                      <option value="openai_audio">{t('channels.protocol.openai_audio')}</option>
                      <option value="openai_speech">{t('channels.protocol.openai_speech')}</option>
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
          {channelType === 'anthropic' && protocol === 'anthropic' ? (
            <div className="grid gap-3 border-l-2 border-warning/30 pl-3">
              <ChannelFormField
                id="channel-client-simulation-profile"
                label={t('channels.fields.clientSimulationProfile')}
                hint={t('channels.form.clientSimulationHint')}
                error={form.formState.errors.clientSimulationProfile?.message}
              >
                <Controller control={form.control} name="clientSimulationProfile" render={({ field }) => (
                  <Select
                    id="channel-client-simulation-profile"
                    value={field.value}
                    onChange={(event) => {
                      field.onChange(event.target.value as ClientSimulationProfile)
                      form.setValue('clientSimulationRiskAccepted', false, {
                        shouldDirty: true,
                        shouldValidate: true,
                      })
                      if (event.target.value !== 'anthropic_cli_headers_v1') {
                        form.setValue('clientSimulationBodyProfile', '', {
                          shouldDirty: true,
                          shouldValidate: true,
                        })
                        form.setValue('clientSimulationBodyRiskAccepted', false, {
                          shouldDirty: true,
                          shouldValidate: true,
                        })
                      }
                    }}
                  >
                    <option value="">{t('channels.clientSimulation.off')}</option>
                    <option value="anthropic_cli_headers_v1">
                      {t('channels.clientSimulation.anthropicCliHeadersV1')}
                    </option>
                  </Select>
                )} />
              </ChannelFormField>
              {clientSimulationProfile ? (
                <div className="flex items-start gap-2 text-xs text-warning">
                  <ShieldAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                  <div className="grid gap-2">
                    <p className="leading-5">{t('channels.form.clientSimulationRisk')}</p>
                    {clientSimulationRiskRequired ? (
                      <label className="flex items-start gap-2 text-foreground">
                        <Controller control={form.control} name="clientSimulationRiskAccepted" render={({ field }) => (
                          <Checkbox
                            checked={field.value}
                            aria-invalid={!!form.formState.errors.clientSimulationRiskAccepted}
                            onCheckedChange={(checked) => field.onChange(checked === true)}
                          />
                        )} />
                        <span>{t('channels.form.clientSimulationRiskAccept')}</span>
                      </label>
                    ) : null}
                    {form.formState.errors.clientSimulationRiskAccepted?.message ? (
                      <p role="alert" className="text-destructive">
                        {form.formState.errors.clientSimulationRiskAccepted.message}
                      </p>
                    ) : null}
                  </div>
                </div>
              ) : null}
              {clientSimulationProfile === 'anthropic_cli_headers_v1' ? (
                <div className="grid gap-3 border-t border-warning/20 pt-3">
                  <ChannelFormField
                    id="channel-client-simulation-body-profile"
                    label={t('channels.fields.clientSimulationBodyProfile')}
                    hint={t('channels.form.clientSimulationBodyHint')}
                    error={form.formState.errors.clientSimulationBodyProfile?.message}
                  >
                    <Controller control={form.control} name="clientSimulationBodyProfile" render={({ field }) => (
                      <Select
                        id="channel-client-simulation-body-profile"
                        value={field.value}
                        onChange={(event) => {
                          field.onChange(event.target.value as ClientSimulationBodyProfile)
                          form.setValue('clientSimulationBodyRiskAccepted', false, {
                            shouldDirty: true,
                            shouldValidate: true,
                          })
                        }}
                      >
                        <option value="">{t('channels.clientSimulation.off')}</option>
                        <option value="anthropic_cli_system_date_v1">
                          {t('channels.clientSimulation.anthropicCliSystemDateV1')}
                        </option>
                      </Select>
                    )} />
                  </ChannelFormField>
                  {clientSimulationBodyProfile ? (
                    <div className="flex items-start gap-2 text-xs text-warning">
                      <ShieldAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
                      <div className="grid gap-2">
                        <p className="leading-5">{t('channels.form.clientSimulationBodyRisk')}</p>
                        {clientSimulationBodyRiskRequired ? (
                          <label className="flex items-start gap-2 text-foreground">
                            <Controller control={form.control} name="clientSimulationBodyRiskAccepted" render={({ field }) => (
                              <Checkbox
                                checked={field.value}
                                aria-invalid={!!form.formState.errors.clientSimulationBodyRiskAccepted}
                                onCheckedChange={(checked) => field.onChange(checked === true)}
                              />
                            )} />
                            <span>{t('channels.form.clientSimulationBodyRiskAccept')}</span>
                          </label>
                        ) : null}
                        {form.formState.errors.clientSimulationBodyRiskAccepted?.message ? (
                          <p role="alert" className="text-destructive">
                            {form.formState.errors.clientSimulationBodyRiskAccepted.message}
                          </p>
                        ) : null}
                      </div>
                    </div>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : null}
          {(channelType === 'openai' || codexOAuth) && protocol === 'openai_responses' ? (
            <div className="grid gap-4 border-l-2 border-primary/20 pl-3">
              {!codexOAuth ? <div className="flex items-center justify-between gap-4">
                <div>
                  <Label htmlFor="channel-responses-websocket">{t('channels.fields.responsesWebsocket')}</Label>
                  <p className="mt-1 text-[0.6875rem] text-muted-foreground">{t('channels.form.responsesWebsocketHint')}</p>
                </div>
                <Controller control={form.control} name="responsesWebsocketEnabled" render={({ field }) => (
                  <Switch id="channel-responses-websocket" checked={field.value} onCheckedChange={field.onChange} />
                )} />
              </div> : null}
              <div className="grid gap-3 sm:grid-cols-[minmax(0,0.7fr)_minmax(0,1.3fr)]">
                <ChannelFormField
                  id="channel-responses-compact-mode"
                  label={t('channels.fields.responsesCompactMode')}
                  hint={t('channels.form.responsesCompactModeHint')}
                  error={form.formState.errors.responsesCompactMode?.message}
                >
                  <Controller control={form.control} name="responsesCompactMode" render={({ field }) => (
                    <Select
                      id="channel-responses-compact-mode"
                      value={field.value}
                      onChange={(event) => field.onChange(event.target.value as ResponsesCompactMode)}
                    >
                      <option value="auto">{t('channels.compactMode.auto')}</option>
                      <option value="force_on">{t('channels.compactMode.forceOn')}</option>
                      <option value="force_off">{t('channels.compactMode.forceOff')}</option>
                    </Select>
                  )} />
                </ChannelFormField>
                <CompactProbeStatus channel={channel} language={i18n.language} />
              </div>
              <ChannelFormField
                id="channel-responses-compact-mapping"
                htmlFor={false}
                label={t('channels.fields.responsesCompactModelMapping')}
                hint={t('channels.form.responsesCompactModelMappingHint')}
                error={form.formState.errors.responsesCompactModelMapping?.message}
              >
                <Controller control={form.control} name="responsesCompactModelMapping" render={({ field }) => (
                  <ChannelKeyValueEditor
                    rows={field.value}
                    keyLabel={t('channels.mapping.source')}
                    valueLabel={t('channels.mapping.target')}
                    addLabel={t('channels.mapping.addCompact')}
                    removeLabel={t('channels.mapping.remove')}
                    emptyText={t('channels.mapping.compactEmpty')}
                    createRow={() => createKeyValueRow('compact-mapping')}
                    onChange={field.onChange}
                  />
                )} />
              </ChannelFormField>
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
          {protocol !== 'openai_embeddings' && protocol !== 'openai_images' && protocol !== 'openai_audio' && protocol !== 'openai_speech' && protocol !== 'jina_rerank' && protocol !== 'cohere_rerank' && protocol !== 'xai_video' ? (
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

function CompactProbeStatus({ channel, language }: { channel?: AdminChannel; language: string }) {
  const { t } = useTranslation()
  const probe = channel?.responses_compact_probe
  const result = probe?.result ?? 'unknown'
  const checkedAt = probe?.checked_at
    ? new Intl.DateTimeFormat(language, { dateStyle: 'medium', timeStyle: 'short' })
      .format(probe.checked_at)
    : undefined
  const tone = result === 'supported'
    ? 'border-success/25 bg-success/10 text-success'
    : result === 'unsupported'
      ? 'border-destructive/25 bg-destructive/10 text-destructive'
      : 'text-muted-foreground'
  return (
    <div className="grid content-start gap-2">
      <Label>{t('channels.fields.responsesCompactProbe')}</Label>
      <div className="flex min-h-9 flex-wrap items-center gap-2">
        <Badge className={tone}>{t(`channels.compactProbe.${result}`)}</Badge>
        {probe?.http_status ? <Badge>HTTP {probe.http_status}</Badge> : null}
        {checkedAt ? <span className="text-[0.6875rem] text-muted-foreground">{checkedAt}</span> : null}
      </div>
    </div>
  )
}
