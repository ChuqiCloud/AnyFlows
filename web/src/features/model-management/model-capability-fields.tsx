import { useTranslation } from 'react-i18next'

import { Switch } from '@/components/ui/switch'
import type { AdminModelModality } from '@/lib/api/generated/types.gen'

const modalities: AdminModelModality[] = ['text', 'image', 'audio', 'video']

type ModelCapabilityFieldsProps = {
  input: AdminModelModality[]
  output: AdminModelModality[]
  supportsReasoning: boolean
  supportsToolCalls: boolean
  onInputChange: (values: AdminModelModality[]) => void
  onOutputChange: (values: AdminModelModality[]) => void
  onReasoningChange: (value: boolean) => void
  onToolCallsChange: (value: boolean) => void
}

/** 以固定模态和能力开关编辑权威声明，避免根据模型名猜测。 */
export function ModelCapabilityFields(props: ModelCapabilityFieldsProps) {
  const { t } = useTranslation()
  const toggle = (
    values: AdminModelModality[],
    modality: AdminModelModality,
    enabled: boolean,
  ) => enabled ? [...values, modality] : values.filter((value) => value !== modality)

  return (
    <div className="grid gap-4">
      <div className="grid gap-3 sm:grid-cols-2">
        <ModalityGroup
          label={t('modelManagement.form.inputModalities')}
          values={props.input}
          onChange={(modality, enabled) => props.onInputChange(toggle(props.input, modality, enabled))}
        />
        <ModalityGroup
          label={t('modelManagement.form.outputModalities')}
          values={props.output}
          onChange={(modality, enabled) => props.onOutputChange(toggle(props.output, modality, enabled))}
        />
      </div>
      <div className="grid gap-2 sm:grid-cols-2">
        <CapabilitySwitch label={t('modelManagement.capabilities.reasoning')} checked={props.supportsReasoning} onCheckedChange={props.onReasoningChange} />
        <CapabilitySwitch label={t('modelManagement.capabilities.toolCalls')} checked={props.supportsToolCalls} onCheckedChange={props.onToolCallsChange} />
      </div>
    </div>
  )
}

function ModalityGroup({ label, values, onChange }: {
  label: string
  values: AdminModelModality[]
  onChange: (modality: AdminModelModality, enabled: boolean) => void
}) {
  const { t } = useTranslation()
  return (
    <fieldset className="rounded-xl border border-[var(--hairline)] p-3">
      <legend className="px-1 text-xs font-semibold">{label}</legend>
      <div className="mt-1 grid gap-2">
        {modalities.map((modality) => (
          <CapabilitySwitch key={modality} label={t(`modelManagement.modalities.${modality}`)} checked={values.includes(modality)} onCheckedChange={(checked) => onChange(modality, checked)} />
        ))}
      </div>
    </fieldset>
  )
}

function CapabilitySwitch({ label, checked, onCheckedChange }: {
  label: string
  checked: boolean
  onCheckedChange: (checked: boolean) => void
}) {
  return (
    <label className="flex min-h-10 items-center justify-between gap-3 rounded-lg bg-surface-2/45 px-3 py-2 text-xs">
      <span>{label}</span>
      <Switch checked={checked} onCheckedChange={onCheckedChange} />
    </label>
  )
}
