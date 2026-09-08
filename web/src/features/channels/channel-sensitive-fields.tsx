import { Controller, type UseFormReturn } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Label } from '@/components/ui/label'
import {
  createKeyValueRow,
  type ChannelEditorMode,
  type ChannelFormValues,
  type SensitiveEditMode,
} from './channel-form-model'
import { ChannelKeyValueEditor } from './channel-key-value-editor'
import { ChannelSensitiveMode } from './channel-sensitive-mode'

type SensitiveFieldProps = {
  mode: ChannelEditorMode
  form: UseFormReturn<ChannelFormValues>
}

/** Header 覆盖使用显式安全语义和逐行编辑器。 */
export function ChannelHeaderOverrideField({ mode, form }: SensitiveFieldProps) {
  const { t } = useTranslation()
  const headerMode = form.watch('headerOverrideMode')
  const error = form.formState.errors.headerOverride?.message
  return (
    <div className="grid gap-2">
      <Label>{t('channels.fields.headerOverride')}</Label>
      {mode === 'update' ? (
        <Controller
          control={form.control}
          name="headerOverrideMode"
          render={({ field }) => (
            <ChannelSensitiveMode
              id="channel-header-mode"
              value={field.value}
              allowPreserve
              onChange={field.onChange}
            />
          )}
        />
      ) : null}
      {headerMode === 'replace' ? (
        <Controller
          control={form.control}
          name="headerOverride"
          render={({ field }) => (
            <ChannelKeyValueEditor
              rows={field.value}
              keyLabel={t('channels.headers.name')}
              valueLabel={t('channels.headers.value')}
              addLabel={t('channels.headers.add')}
              removeLabel={t('channels.headers.remove')}
              emptyText={t('channels.headers.empty')}
              createRow={() => createKeyValueRow('header')}
              onChange={field.onChange}
            />
          )}
        />
      ) : <SensitiveModeHint mode={headerMode} />}
      {error ? (
        <p className="text-xs text-destructive">{error}</p>
      ) : (
        <p className="text-[0.6875rem] leading-4 text-muted-foreground">{t('channels.form.headersHint')}</p>
      )}
    </div>
  )
}

function SensitiveModeHint({ mode }: { mode: SensitiveEditMode }) {
  const { t } = useTranslation()
  const tone = mode === 'clear'
    ? 'bg-warning/10 text-warning'
    : 'bg-surface-2 text-muted-foreground'
  return <p className={`rounded-lg px-3 py-2 text-xs ${tone}`}>{t(`channels.sensitiveMode.${mode}Hint`)}</p>
}
