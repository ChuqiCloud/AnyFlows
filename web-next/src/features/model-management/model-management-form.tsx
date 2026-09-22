import { ProviderPicker } from '@/components/brand/provider-picker'
import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Button, Input, Select, SelectItem, Textarea } from '@heroui/react'
import { LoaderCircle } from 'lucide-react'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import type { AdminModel } from '@/lib/api/generated/types.gen'
import { isModelConflict, useCreateAdminModel, useUpdateAdminModel } from './model-management-api'
import { ModelCapabilityFields } from './model-capability-fields'
import { ModelManagementField } from './model-management-field'
import {
  buildModelManagementSchema,
  defaultModelManagementValues,
  toModelCreateRequest,
  toModelUpdateRequest,
  type ModelEditorMode,
  type ModelManagementFormValues,
} from './model-management-form-model'
import { ModelTagEditor } from './model-tag-editor'

type ModelManagementFormProps = {
  mode: ModelEditorMode
  model?: AdminModel
  onCancel: () => void
  onSaved: (model: AdminModel) => void
}

export function ModelManagementForm({ mode, model, onCancel, onSaved }: ModelManagementFormProps) {
  const { t } = useTranslation()
  const createMutation = useCreateAdminModel()
  const updateMutation = useUpdateAdminModel()
  const schema = buildModelManagementSchema({
    model: t('modelManagement.validation.model'),
    displayName: t('modelManagement.validation.displayName'),
    provider: t('modelManagement.validation.provider'),
    description: t('modelManagement.validation.description'),
    iconUrl: t('modelManagement.validation.iconUrl'),
    tags: t('modelManagement.validation.tags'),
    contextWindow: t('modelManagement.validation.contextWindow'),
    modalities: t('modelManagement.validation.modalities'),
  })
  const form = useForm<ModelManagementFormValues>({
    defaultValues: defaultModelManagementValues(model),
    resolver: zodResolver(schema),
  })

  useEffect(() => {
    form.reset(defaultModelManagementValues(model))
  }, [form, mode, model])

  const pending = createMutation.isPending || updateMutation.isPending
  const submitError = createMutation.error ?? updateMutation.error
  const inputModalities = form.watch('inputModalities')
  const outputModalities = form.watch('outputModalities')
  const tags = form.watch('tags')
  const supportsReasoning = form.watch('supportsReasoning')
  const supportsToolCalls = form.watch('supportsToolCalls')

  const submit = form.handleSubmit(async (values) => {
    try {
      const saved = mode === 'create'
        ? await createMutation.mutateAsync(toModelCreateRequest(values))
        : model
          ? await updateMutation.mutateAsync({ id: model.id, body: toModelUpdateRequest(values) })
          : undefined
      if (saved) onSaved(saved)
    } catch {
      // Mutation 状态保留结构化错误，表单内容保持不变供管理员修正。
    }
  })

  return (
    <form className="flex min-h-0 flex-1 flex-col" onSubmit={submit} noValidate>
      <div className="flex-1 space-y-6 overflow-y-auto px-4 pt-4 pb-5">
        <section className="grid gap-3" aria-labelledby="model-identity-fields">
          <h3 id="model-identity-fields" className="text-xs font-semibold">{t('modelManagement.form.identity')}</h3>
          <ModelManagementField id="model-canonical" label={t('modelManagement.fields.model')} hint={t(mode === 'create' ? 'modelManagement.form.modelCreateHint' : 'modelManagement.form.modelImmutableHint')} error={form.formState.errors.model?.message}>
            <Input id="model-canonical" isDisabled={mode === 'update'} autoComplete="off" isInvalid={!!form.formState.errors.model} size="sm" {...form.register('model')} />
          </ModelManagementField>
          <div className="grid gap-3 sm:grid-cols-2">
            <ModelManagementField id="model-display-name" label={t('modelManagement.fields.displayName')} error={form.formState.errors.displayName?.message}>
              <Input id="model-display-name" autoComplete="off" isInvalid={!!form.formState.errors.displayName} size="sm" {...form.register('displayName')} />
            </ModelManagementField>
            <ModelManagementField id="model-provider" label={t('modelManagement.fields.provider')} hint={t('modelManagement.form.providerHint')} error={form.formState.errors.provider?.message}>
              <Controller control={form.control} name="provider" render={({ field }) => <ProviderPicker id="model-provider" {...field} invalid={!!form.formState.errors.provider} disabled={pending} />} />
            </ModelManagementField>
          </div>
          <ModelManagementField id="model-description" label={t('modelManagement.fields.description')} hint={t('modelManagement.form.optionalHint')} error={form.formState.errors.description?.message}>
            <Textarea id="model-description" isInvalid={!!form.formState.errors.description} minRows={4} size="sm" {...form.register('description')} />
          </ModelManagementField>
          <ModelManagementField id="model-icon-url" label={t('modelManagement.fields.iconUrl')} hint={t('modelManagement.form.iconHint')} error={form.formState.errors.iconUrl?.message}>
            <Input id="model-icon-url" type="url" autoComplete="off" isInvalid={!!form.formState.errors.iconUrl} size="sm" {...form.register('iconUrl')} />
          </ModelManagementField>
          <ModelManagementField id="model-tags" label={t('modelManagement.fields.tags')} hint={t('modelManagement.form.tagsHint')} error={form.formState.errors.tags?.message}>
            <ModelTagEditor id="model-tags" tags={tags} disabled={pending} onChange={(values) => form.setValue('tags', values, { shouldDirty: true, shouldValidate: true })} />
          </ModelManagementField>
        </section>

        <section className="grid gap-3 border-t border-[var(--hairline)] pt-5" aria-labelledby="model-capability-fields">
          <div>
            <h3 id="model-capability-fields" className="text-xs font-semibold">{t('modelManagement.form.capabilities')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.form.capabilitiesHint')}</p>
          </div>
          <ModelManagementField id="model-context-window" label={t('modelManagement.fields.contextWindow')} hint={t('modelManagement.form.contextHint')} error={form.formState.errors.contextWindow?.message}>
            <Input id="model-context-window" type="number" min={1} max={2_147_483_647} step={1} inputMode="numeric" isInvalid={!!form.formState.errors.contextWindow} size="sm" {...form.register('contextWindow')} />
          </ModelManagementField>
          <ModelCapabilityFields
            input={inputModalities}
            output={outputModalities}
            supportsReasoning={supportsReasoning}
            supportsToolCalls={supportsToolCalls}
            onInputChange={(values) => form.setValue('inputModalities', values, { shouldDirty: true, shouldValidate: true })}
            onOutputChange={(values) => form.setValue('outputModalities', values, { shouldDirty: true, shouldValidate: true })}
            onReasoningChange={(value) => form.setValue('supportsReasoning', value, { shouldDirty: true })}
            onToolCallsChange={(value) => form.setValue('supportsToolCalls', value, { shouldDirty: true })}
          />
          {(form.formState.errors.inputModalities || form.formState.errors.outputModalities) ? (
            <p role="alert" className="text-xs text-destructive">{t('modelManagement.validation.modalities')}</p>
          ) : null}
        </section>

        <section className="grid gap-3 border-t border-[var(--hairline)] pt-5" aria-labelledby="model-operation-fields">
          <h3 id="model-operation-fields" className="text-xs font-semibold">{t('modelManagement.form.operation')}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <ModelManagementField id="model-visibility" label={t('modelManagement.fields.visibility')} hint={t('modelManagement.form.visibilityHint')}>
              <Controller
                control={form.control}
                name="visibility"
                render={({ field }) => (
                  <Select aria-label={t('modelManagement.fields.visibility')} selectedKeys={[field.value]} size="sm" onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? ''))}>
                    <SelectItem key="public">{t('modelManagement.visibility.public')}</SelectItem>
                    <SelectItem key="authenticated">{t('modelManagement.visibility.authenticated')}</SelectItem>
                    <SelectItem key="hidden">{t('modelManagement.visibility.hidden')}</SelectItem>
                  </Select>
                )}
              />
            </ModelManagementField>
            <ModelManagementField id="model-lifecycle" label={t('modelManagement.fields.lifecycle')} hint={t('modelManagement.form.lifecycleHint')}>
              <Controller
                control={form.control}
                name="lifecycle"
                render={({ field }) => (
                  <Select aria-label={t('modelManagement.fields.lifecycle')} selectedKeys={[field.value]} size="sm" onSelectionChange={(keys) => field.onChange(String(Array.from(keys)[0] ?? ''))}>
                    <SelectItem key="draft">{t('modelManagement.lifecycle.draft')}</SelectItem>
                    <SelectItem key="active">{t('modelManagement.lifecycle.active')}</SelectItem>
                    <SelectItem key="deprecated">{t('modelManagement.lifecycle.deprecated')}</SelectItem>
                    <SelectItem key="retired">{t('modelManagement.lifecycle.retired')}</SelectItem>
                  </Select>
                )}
              />
            </ModelManagementField>
          </div>
        </section>

        {submitError ? (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {t(isModelConflict(submitError) ? 'modelManagement.form.conflict' : 'modelManagement.form.submitError')}
          </p>
        ) : null}
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="bordered" onClick={onCancel}>{t('modelManagement.actions.cancel')}</Button>
        <Button type="submit" color="primary" isDisabled={pending}>
          {pending ? <LoaderCircle className="size-4 animate-spin" aria-hidden="true" /> : null}
          {t(mode === 'create' ? 'modelManagement.actions.create' : 'modelManagement.actions.save')}
        </Button>
      </div>
    </form>
  )
}
