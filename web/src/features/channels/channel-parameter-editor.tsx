import { Plus, Trash2 } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Select } from '@/components/ui/select'
import { createParameterRow, type ParameterRow, type ParameterValueType } from './channel-form-model'

type ChannelParameterEditorProps = {
  rows: ParameterRow[]
  onChange: (rows: ParameterRow[]) => void
}

const parameterTypes: ParameterValueType[] = ['string', 'number', 'boolean', 'null', 'json']

/** 参数覆盖按值类型编辑，避免把简单值藏进完整 JSON 对象。 */
export function ChannelParameterEditor({ rows, onChange }: ChannelParameterEditorProps) {
  const { t } = useTranslation()
  const updateRow = (id: string, patch: Partial<ParameterRow>) => {
    onChange(rows.map((row) => row.id === id ? { ...row, ...patch } : row))
  }

  return (
    <div className="grid gap-2">
      {rows.length > 0 ? rows.map((row) => (
        <div key={row.id} className="grid grid-cols-[minmax(0,1fr)_7rem_2rem] gap-2">
          <Input
            value={row.key}
            placeholder={t('channels.parameters.name')}
            aria-label={t('channels.parameters.name')}
            onChange={(event) => updateRow(row.id, { key: event.target.value })}
          />
          <Select
            value={row.type}
            aria-label={t('channels.parameters.type')}
            onChange={(event) => {
              const type = event.target.value as ParameterValueType
              updateRow(row.id, {
                type,
                value: type === 'boolean' ? 'true' : type === 'null' ? '' : row.value,
              })
            }}
          >
            {parameterTypes.map((type) => (
              <option key={type} value={type}>{t(`channels.parameters.types.${type}`)}</option>
            ))}
          </Select>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            className="text-muted-foreground hover:text-destructive"
            aria-label={t('channels.parameters.remove', { name: row.key || t('channels.parameters.name') })}
            title={t('channels.parameters.remove', { name: row.key || t('channels.parameters.name') })}
            onClick={() => onChange(rows.filter((item) => item.id !== row.id))}
          >
            <Trash2 aria-hidden="true" />
          </Button>
          <div className="col-span-3">
            {row.type === 'boolean' ? (
              <Select
                value={row.value}
                aria-label={t('channels.parameters.value')}
                onChange={(event) => updateRow(row.id, { value: event.target.value })}
              >
                <option value="true">true</option>
                <option value="false">false</option>
              </Select>
            ) : row.type === 'null' ? (
              <div className="flex h-8 items-center rounded-lg border border-input bg-[var(--surface-sunken)] px-2.5 font-mono text-xs text-muted-foreground">
                null
              </div>
            ) : (
              <Input
                className={row.type === 'json' ? 'font-mono text-xs' : undefined}
                inputMode={row.type === 'number' ? 'decimal' : undefined}
                value={row.value}
                placeholder={row.type === 'json' ? '{"key":"value"}' : t('channels.parameters.value')}
                aria-label={t('channels.parameters.value')}
                onChange={(event) => updateRow(row.id, { value: event.target.value })}
              />
            )}
          </div>
        </div>
      )) : (
        <p className="rounded-lg border border-dashed border-[var(--hairline)] px-3 py-4 text-center text-xs text-muted-foreground">
          {t('channels.parameters.empty')}
        </p>
      )}
      <Button type="button" size="sm" variant="secondary" className="w-fit" onClick={() => onChange([...rows, createParameterRow()])}>
        <Plus aria-hidden="true" />
        {t('channels.parameters.add')}
      </Button>
    </div>
  )
}
