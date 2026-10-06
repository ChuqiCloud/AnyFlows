import { Button, Input } from '@heroui/react'
import { Plus, Trash2 } from 'lucide-react'

import type { KeyValueRow } from './channel-form-model'

type ChannelKeyValueEditorProps = {
  rows: KeyValueRow[]
  keyLabel: string
  valueLabel: string
  addLabel: string
  removeLabel: string
  emptyText: string
  onChange: (rows: KeyValueRow[]) => void
  createRow: () => KeyValueRow
}

/** 以逐行键值控件替代整段 JSON 对象输入。 */
export function ChannelKeyValueEditor({
  rows,
  keyLabel,
  valueLabel,
  addLabel,
  removeLabel,
  emptyText,
  onChange,
  createRow,
}: ChannelKeyValueEditorProps) {
  const updateRow = (id: string, patch: Partial<KeyValueRow>) => {
    onChange(rows.map((row) => row.id === id ? { ...row, ...patch } : row))
  }

  return (
    <div className="grid gap-2">
      {rows.length > 0 ? (
        <div className="grid gap-2">
          <div className="hidden grid-cols-[minmax(0,1fr)_minmax(0,1fr)_2rem] gap-2 px-1 text-[0.6875rem] text-muted-foreground sm:grid">
            <span>{keyLabel}</span>
            <span>{valueLabel}</span>
          </div>
          {rows.map((row) => (
            <div key={row.id} className="grid grid-cols-[minmax(0,1fr)_2rem] gap-2 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_2rem]">
              <Input
                aria-label={keyLabel}
                placeholder={keyLabel}
                size="sm"
                value={row.key}
                onChange={(event) => updateRow(row.id, { key: event.target.value })}
              />
              <Input
                aria-label={valueLabel}
                className="col-start-1 sm:col-start-2 sm:row-start-1"
                classNames={{ input: 'font-mono text-xs' }}
                placeholder={valueLabel}
                size="sm"
                value={row.value}
                onChange={(event) => updateRow(row.id, { value: event.target.value })}
              />
              <Button
                isIconOnly
                aria-label={`${removeLabel}: ${row.key || keyLabel}`}
                className="col-start-2 row-start-1 text-muted-foreground hover:text-destructive sm:col-start-3"
                size="sm"
                title={`${removeLabel}: ${row.key || keyLabel}`}
                type="button"
                variant="light"
                onClick={() => onChange(rows.filter((item) => item.id !== row.id))}
              >
                <Trash2 className="size-3.5" aria-hidden="true" />
              </Button>
            </div>
          ))}
        </div>
      ) : (
        <p className="rounded-lg border border-dashed border-[var(--hairline)] px-3 py-4 text-center text-xs text-muted-foreground">
          {emptyText}
        </p>
      )}
      <Button type="button" size="sm" variant="bordered" className="w-fit" onClick={() => onChange([...rows, createRow()])}>
        <Plus className="size-3.5" aria-hidden="true" />
        {addLabel}
      </Button>
    </div>
  )
}
