import { useEffect } from 'react'
import { zodResolver } from '@hookform/resolvers/zod'
import { Controller, useForm } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { ProviderPicker } from '@/components/brand/provider-picker'
import { Input } from '@/components/ui/input'
import { Sheet, SheetContent, SheetDescription, SheetHeader, SheetTitle } from '@/components/ui/sheet'
import type { AdminMissingModel } from '@/lib/api/generated/types.gen'
import { ModelCapabilityFields } from './model-capability-fields'
import { ModelManagementField } from './model-management-field'
import {
  validModelText,
  type ModelValidationMessages,
} from './model-management-form-model'
import {
  buildMissingModelImportSchema,
  defaultMissingModelImportDraft,
  missingDisplayNamesAreValid,
  type MissingModelImportDraft,
} from './missing-model-form-model'

type MissingModelImportSheetProps = {
  displayNames: Record<string, string>
  errorCode?: string
  models: AdminMissingModel[]
  open: boolean
  pending: boolean
  onDisplayNameChange: (model: string, displayName: string) => void
  onImport: (draft: MissingModelImportDraft) => void
  onOpenChange: (open: boolean) => void
}

/** 复核批次共享能力与逐项展示名，不提供自动猜测入口。 */
export function MissingModelImportSheet(props: MissingModelImportSheetProps) {
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
  const form = useForm<MissingModelImportDraft>({
    defaultValues: defaultMissingModelImportDraft(),
    mode: 'onChange',
    resolver: zodResolver(buildMissingModelImportSchema(messages)),
  })
  useEffect(() => {
    if (props.open) form.reset(defaultMissingModelImportDraft())
  }, [form, props.open])

  const inputModalities = form.watch('inputModalities')
  const outputModalities = form.watch('outputModalities')
  const namesValid = missingDisplayNamesAreValid(props.models, props.displayNames)

  return (
    <Sheet open={props.open} onOpenChange={props.onOpenChange}>
      <SheetContent className="gap-0 data-[side=right]:w-full data-[side=right]:sm:max-w-2xl" aria-describedby="missing-model-import-description">
        <SheetHeader className="border-b border-[var(--hairline)] pr-12">
          <SheetTitle>{t('modelManagement.missing.import.title')}</SheetTitle>
          <SheetDescription id="missing-model-import-description">
            {t('modelManagement.missing.import.description', { count: props.models.length })}
          </SheetDescription>
        </SheetHeader>
        <form className="flex min-h-0 flex-1 flex-col" noValidate onSubmit={form.handleSubmit(props.onImport)}>
          <div className="flex-1 space-y-5 overflow-y-auto px-4 py-5">
            <section className="grid gap-3" aria-labelledby="missing-model-identities">
              <div>
                <h3 id="missing-model-identities" className="text-xs font-semibold">{t('modelManagement.missing.import.identities')}</h3>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.missing.import.identityHint')}</p>
              </div>
              <div className="grid gap-2">
                {props.models.map((model) => {
                  const value = props.displayNames[model.model] ?? model.model
                  const invalid = !validModelText(value, 128)
                  return (
                    <div key={model.model} className="grid gap-2 rounded-lg border border-[var(--hairline)] p-3 sm:grid-cols-[minmax(0,0.8fr)_minmax(0,1fr)] sm:items-center">
                      <div className="min-w-0 truncate font-mono text-xs" title={model.model}>{model.model}</div>
                      <Input
                        value={value}
                        aria-label={t('modelManagement.missing.import.displayNameFor', { model: model.model })}
                        aria-invalid={invalid}
                        onChange={(event) => props.onDisplayNameChange(model.model, event.target.value)}
                      />
                    </div>
                  )
                })}
              </div>
              {!namesValid ? <p role="alert" className="text-xs text-destructive">{t('modelManagement.validation.displayName')}</p> : null}
            </section>

            <section className="grid gap-3 border-t border-[var(--hairline)] pt-5" aria-labelledby="missing-model-shared-fields">
              <div>
                <h3 id="missing-model-shared-fields" className="text-xs font-semibold">{t('modelManagement.missing.import.shared')}</h3>
                <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('modelManagement.missing.import.sharedHint')}</p>
              </div>
              <ModelManagementField id="missing-provider" label={t('modelManagement.fields.provider')} error={form.formState.errors.provider?.message}>
                <Controller control={form.control} name="provider" render={({ field }) => <ProviderPicker id="missing-provider" {...field} invalid={!!form.formState.errors.provider} disabled={props.pending} />} />
              </ModelManagementField>
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

            <p className="rounded-lg bg-surface-2/60 px-3 py-2 text-xs leading-5 text-muted-foreground">
              {t('modelManagement.missing.import.boundary')}
            </p>
          </div>
          <div className="grid gap-3 border-t border-[var(--hairline)] p-4">
            {props.errorCode ? (
              <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
                {t(`modelManagement.missing.import.errors.${props.errorCode}`, {
                  defaultValue: t('modelManagement.missing.import.errors.unknown'),
                })}
              </p>
            ) : null}
            <div className="flex justify-end gap-2">
              <Button type="button" variant="secondary" disabled={props.pending} onClick={() => props.onOpenChange(false)}>{t('modelManagement.actions.cancel')}</Button>
              <Button type="submit" disabled={props.pending || !namesValid || !form.formState.isValid}>{t(props.pending ? 'modelManagement.missing.import.importing' : 'modelManagement.missing.import.submit')}</Button>
            </div>
          </div>
        </form>
      </SheetContent>
    </Sheet>
  )
}
