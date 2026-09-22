import { ProviderPicker } from '@/components/brand/provider-picker'
import { Button, Input, Textarea } from '@heroui/react'
import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import type { AdminModelSyncPreviewItem } from '@/lib/api/generated/types.gen'
import { ModelCapabilityFields } from './model-capability-fields'
import {
  type ModelValidationMessages,
} from './model-management-form-model'
import { ModelManagementField } from './model-management-field'
import { ModelSyncEvidence } from './model-sync-evidence'
import {
  buildModelSyncDraftSchema,
  type ModelSyncDraft,
} from './model-sync-form-model'
import { ModelTagEditor } from './model-tag-editor'

type ModelSyncItemFormProps = {
  draft: ModelSyncDraft
  item: AdminModelSyncPreviewItem
  onCancel: () => void
  onSave: (draft: ModelSyncDraft) => void
}

export function ModelSyncItemForm({ draft, item, onCancel, onSave }: ModelSyncItemFormProps) {
  const { t } = useTranslation()
  const messages: ModelValidationMessages = {
    model: t('modelManagement.validation.model'),
    displayName: t('modelManagement.validation.displayName'),
    provider: t('modelManagement.validation.provider'),
    description: t('modelManagement.validation.description'),
    iconUrl: t('modelManagement.validation.iconUrl'),
    tags: t('modelManagement.validation.tags'),
    contextWindow: t('modelManagement.validation.contextWindow'),
    modalities: t('modelManagement.validation.modalities'),
  }
  const form = useForm<ModelSyncDraft>({
    defaultValues: draft,
    resolver: zodResolver(buildModelSyncDraftSchema(messages)),
  })

  useEffect(() => form.reset(draft), [draft, form, item.item_id])

  const inputModalities = form.watch('inputModalities')
  const outputModalities = form.watch('outputModalities')
  const tags = form.watch('tags')

  return (
    <form className="flex min-h-0 flex-1 flex-col" noValidate onSubmit={form.handleSubmit(onSave)}>
      <div className="flex-1 space-y-5 overflow-y-auto px-4 pt-4 pb-5">
        <ModelSyncEvidence
          item={item}
          onUseDisplayName={() => form.setValue('displayName', item.display_name_hint ?? '', { shouldDirty: true, shouldValidate: true })}
          onUseDescription={() => form.setValue('description', item.description_hint ?? '', { shouldDirty: true, shouldValidate: true })}
          onUseContext={() => form.setValue('contextWindow', item.context_window_hint === null ? '' : String(item.context_window_hint), { shouldDirty: true, shouldValidate: true })}
        />

        <section className="grid gap-3" aria-labelledby="model-sync-authority-fields">
          <div>
            <h3 id="model-sync-authority-fields" className="text-xs font-semibold">{t('modelManagement.sync.form.authority')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.sync.form.authorityHint')}</p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <ModelManagementField id="sync-display-name" label={t('modelManagement.fields.displayName')} error={form.formState.errors.displayName?.message}>
              <Input id="sync-display-name" size="sm" autoComplete="off" isInvalid={!!form.formState.errors.displayName} {...form.register('displayName')} />
            </ModelManagementField>
            <ModelManagementField id="sync-provider" label={t('modelManagement.fields.provider')} error={form.formState.errors.provider?.message}>
              <Controller control={form.control} name="provider" render={({ field }) => <ProviderPicker id="sync-provider" {...field} invalid={!!form.formState.errors.provider} />} />
            </ModelManagementField>
          </div>
          <ModelManagementField id="sync-description" label={t('modelManagement.fields.description')} error={form.formState.errors.description?.message}>
            <Textarea id="sync-description" size="sm" minRows={3} isInvalid={!!form.formState.errors.description} {...form.register('description')} />
          </ModelManagementField>
          <div className="grid gap-3 sm:grid-cols-2">
            <ModelManagementField id="sync-icon-url" label={t('modelManagement.fields.iconUrl')} error={form.formState.errors.iconUrl?.message}>
              <Input id="sync-icon-url" size="sm" type="url" autoComplete="off" isInvalid={!!form.formState.errors.iconUrl} {...form.register('iconUrl')} />
            </ModelManagementField>
            <ModelManagementField id="sync-context-window" label={t('modelManagement.fields.contextWindow')} error={form.formState.errors.contextWindow?.message}>
              <Input id="sync-context-window" size="sm" type="number" min={1} max={2_147_483_647} step={1} inputMode="numeric" isInvalid={!!form.formState.errors.contextWindow} {...form.register('contextWindow')} />
            </ModelManagementField>
          </div>
          <ModelManagementField id="sync-tags" label={t('modelManagement.fields.tags')} error={form.formState.errors.tags?.message}>
            <ModelTagEditor id="sync-tags" tags={tags} onChange={(values) => form.setValue('tags', values, { shouldDirty: true, shouldValidate: true })} />
          </ModelManagementField>
        </section>

        <section className="grid gap-3 border-t border-[var(--hairline)] pt-5" aria-labelledby="model-sync-capability-fields">
          <div>
            <h3 id="model-sync-capability-fields" className="text-xs font-semibold">{t('modelManagement.form.capabilities')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.sync.form.capabilitiesHint')}</p>
          </div>
          <ModelCapabilityFields
            input={inputModalities}
            output={outputModalities}
            supportsReasoning={form.watch('supportsReasoning')}
            supportsToolCalls={form.watch('supportsToolCalls')}
            onInputChange={(values) => form.setValue('inputModalities', values, { shouldDirty: true, shouldValidate: true })}
            onOutputChange={(values) => form.setValue('outputModalities', values, { shouldDirty: true, shouldValidate: true })}
            onReasoningChange={(value) => form.setValue('supportsReasoning', value, { shouldDirty: true })}
            onToolCallsChange={(value) => form.setValue('supportsToolCalls', value, { shouldDirty: true })}
          />
          {(form.formState.errors.inputModalities || form.formState.errors.outputModalities) ? (
            <p role="alert" className="text-xs text-destructive">{t('modelManagement.validation.modalities')}</p>
          ) : null}
        </section>
      </div>

      <div className="flex justify-end gap-2 border-t border-[var(--hairline)] p-4">
        <Button type="button" variant="bordered" onClick={onCancel}>{t('modelManagement.actions.cancel')}</Button>
        <Button type="submit" color="primary">{t('modelManagement.sync.actions.confirmItem')}</Button>
      </div>
    </form>
  )
}
