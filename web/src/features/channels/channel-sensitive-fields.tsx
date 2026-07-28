import { Braces, ChevronDown } from 'lucide-react'
import { Controller, type UseFormReturn } from 'react-hook-form'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { ChannelFormField } from './channel-form-field'
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

/** 适配器专有 JSON 默认折叠，避免干扰常用渠道配置。 */
export function ChannelAdvancedSettings({ mode, form }: SensitiveFieldProps) {
  const { t } = useTranslation()
  const settingsMode = form.watch('settingsMode')
  return (
    <details className="group rounded-xl border border-[var(--hairline)] bg-surface-1 [&_summary::-webkit-details-marker]:hidden">
      <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-3 text-xs font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <Braces className="size-3.5 text-muted-foreground" aria-hidden="true" />
        <span className="flex-1">{t('channels.form.advanced')}</span>
        <Badge className="border-transparent text-muted-foreground">JSON</Badge>
        <ChevronDown className="size-3.5 text-muted-foreground transition-transform group-open:rotate-180" aria-hidden="true" />
      </summary>
      <div className="grid gap-3 border-t border-[var(--hairline)] p-3">
        {mode === 'update' ? (
          <Controller
            control={form.control}
            name="settingsMode"
            render={({ field }) => (
              <ChannelSensitiveMode
                id="channel-settings-mode"
                value={field.value}
                allowPreserve
                onChange={field.onChange}
              />
            )}
          />
        ) : null}
        {settingsMode === 'replace' ? (
          <ChannelFormField
            id="channel-settings"
            label={t('channels.fields.settings')}
            error={form.formState.errors.settings?.message}
            hint={t('channels.form.settingsHint')}
          >
            <Textarea
              id="channel-settings"
              className="min-h-28 font-mono text-xs"
              aria-invalid={!!form.formState.errors.settings}
              {...form.register('settings')}
            />
          </ChannelFormField>
        ) : <SensitiveModeHint mode={settingsMode} />}
      </div>
    </details>
  )
}

function SensitiveModeHint({ mode }: { mode: SensitiveEditMode }) {
  const { t } = useTranslation()
  const tone = mode === 'clear'
    ? 'bg-warning/10 text-warning'
    : 'bg-surface-2 text-muted-foreground'
  return <p className={`rounded-lg px-3 py-2 text-xs ${tone}`}>{t(`channels.sensitiveMode.${mode}Hint`)}</p>
}
