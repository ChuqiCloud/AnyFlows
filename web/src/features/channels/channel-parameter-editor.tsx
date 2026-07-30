import { Plus, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import {
  channelParameterNames,
  createParameterRow,
  type ChannelProtocol,
  type ChannelParameterName,
  type ParameterRow,
} from './channel-form-model'

type ChannelParameterEditorProps = {
  protocol: ChannelProtocol
  rows: ParameterRow[]
  onChange: (rows: ParameterRow[]) => void
}

type NumericParameterName = Exclude<ChannelParameterName, 'stop_sequences'>

const numericParameterLimits = {
  temperature: { min: 0, max: 2, step: 0.01 },
  top_p: { min: 0, max: 1, step: 0.01 },
  max_output_tokens: { min: 1, max: 1_000_000, step: 1 },
} as const

/** 使用闭合参数控件编辑覆盖策略，避免让用户手写 JSON。 */
export function ChannelParameterEditor({ protocol, rows, onChange }: ChannelParameterEditorProps) {
  const { t } = useTranslation()
  const availableParameters = channelParameterNames.filter(
    (name) => protocol !== 'openai_responses' || name !== 'stop_sequences',
  )
  const nextParameter = availableParameters.find((name) => !rows.some((row) => row.key === name))
  const updateRow = (id: string, patch: Partial<ParameterRow>) => {
    onChange(rows.map((row) => row.id === id ? { ...row, ...patch } : row))
  }

  const updateParameterName = (row: ParameterRow, key: ChannelParameterName) => {
    updateRow(row.id, {
      key,
      value: '',
      values: key === 'stop_sequences' ? [''] : [],
    })
  }

  return (
    <div className="grid gap-3">
      {rows.length > 0 ? (
        <div className="grid gap-3">
          {rows.map((row) => (
            <div key={row.id} className="grid gap-2 border-b border-[var(--hairline)] pb-3 last:border-b-0 last:pb-0">
              <div className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2">
                <Select
                  value={row.key}
                  aria-label={t('channels.parameters.name')}
                  onChange={(event) => updateParameterName(row, event.target.value as ChannelParameterName)}
                >
                  <option value="" disabled>{t('channels.parameters.name')}</option>
                  {availableParameters.map((name) => (
                    <option
                      key={name}
                      value={name}
                      disabled={name !== row.key && rows.some((item) => item.key === name)}
                    >
                      {t(`channels.parameters.names.${name}`)}
                    </option>
                  ))}
                </Select>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  className="text-muted-foreground hover:text-destructive"
                  aria-label={t('channels.parameters.remove', {
                    name: row.key ? t(`channels.parameters.names.${row.key}`) : t('channels.parameters.name'),
                  })}
                  title={t('channels.parameters.remove', {
                    name: row.key ? t(`channels.parameters.names.${row.key}`) : t('channels.parameters.name'),
                  })}
                  onClick={() => onChange(rows.filter((item) => item.id !== row.id))}
                >
                  <Trash2 aria-hidden="true" />
                </Button>
              </div>

              {row.key === 'stop_sequences' ? (
                <StopSequenceInputs row={row} onChange={(values) => updateRow(row.id, { values })} />
              ) : isNumericParameterName(row.key) ? (
                <NumericParameterInput
                  protocol={protocol}
                  name={row.key}
                  value={row.value}
                  onChange={(value) => updateRow(row.id, { value })}
                />
              ) : null}
            </div>
          ))}
        </div>
      ) : (
        <p className="rounded-lg border border-dashed border-[var(--hairline)] px-3 py-4 text-center text-xs text-muted-foreground">
          {t('channels.parameters.empty')}
        </p>
      )}
      <Button
        type="button"
        size="sm"
        variant="secondary"
        className="w-fit"
        disabled={!nextParameter}
        onClick={() => {
          if (nextParameter) onChange([...rows, createParameterRow(nextParameter)])
        }}
      >
        <Plus aria-hidden="true" />
        {t('channels.parameters.add')}
      </Button>
    </div>
  )
}

function isNumericParameterName(name: ChannelParameterName | ''): name is NumericParameterName {
  return name !== '' && name !== 'stop_sequences'
}

function NumericParameterInput({ protocol, name, value, onChange }: {
  protocol: ChannelProtocol
  name: NumericParameterName
  value: string
  onChange: (value: string) => void
}) {
  const { t } = useTranslation()
  const limits = name === 'temperature' && protocol === 'anthropic'
    ? { ...numericParameterLimits.temperature, max: 1 }
    : numericParameterLimits[name]
  const rangeKey = name === 'temperature' && protocol === 'anthropic'
    ? 'temperatureAnthropic'
    : name

  return (
    <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-2">
      <Input
        type="number"
        inputMode={name === 'max_output_tokens' ? 'numeric' : 'decimal'}
        min={limits.min}
        max={limits.max}
        step={limits.step}
        value={value}
        placeholder={t(`channels.parameters.placeholders.${name}`)}
        aria-label={t('channels.parameters.valueFor', {
          name: t(`channels.parameters.names.${name}`),
        })}
        onChange={(event) => onChange(event.target.value)}
      />
      <Badge className="whitespace-nowrap text-muted-foreground">
        {t(`channels.parameters.ranges.${rangeKey}`)}
      </Badge>
    </div>
  )
}

type StopSequenceInputsProps = {
  row: ParameterRow
  onChange: (values: string[]) => void
}

/** 将停止序列拆成独立输入，并保持一至四条的稳定边界。 */
function StopSequenceInputs({ row, onChange }: StopSequenceInputsProps) {
  const { t } = useTranslation()
  const updateValue = (index: number, value: string) => {
    onChange(row.values.map((item, itemIndex) => itemIndex === index ? value : item))
  }

  return (
    <div className="grid gap-2">
      {row.values.map((value, index) => (
        <div key={index} className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2">
          <Input
            value={value}
            placeholder={t('channels.parameters.stopSequence', { index: index + 1 })}
            aria-label={t('channels.parameters.stopSequence', { index: index + 1 })}
            onChange={(event) => updateValue(index, event.target.value)}
          />
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            className="text-muted-foreground hover:text-destructive"
            disabled={row.values.length === 1}
            aria-label={t('channels.parameters.removeStopSequence', { index: index + 1 })}
            title={t('channels.parameters.removeStopSequence', { index: index + 1 })}
            onClick={() => onChange(row.values.filter((_, itemIndex) => itemIndex !== index))}
          >
            <Trash2 aria-hidden="true" />
          </Button>
        </div>
      ))}
      <Button
        type="button"
        size="xs"
        variant="ghost"
        className="w-fit"
        disabled={row.values.length >= 4}
        onClick={() => onChange([...row.values, ''])}
      >
        <Plus aria-hidden="true" />
        {t('channels.parameters.addStopSequence')}
      </Button>
    </div>
  )
}
