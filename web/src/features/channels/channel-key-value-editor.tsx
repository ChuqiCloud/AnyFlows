import { Plus, Trash2 } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
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
                value={row.key}
                placeholder={keyLabel}
                aria-label={keyLabel}
                onChange={(event) => updateRow(row.id, { key: event.target.value })}
              />
              <Input
                className="col-start-1 font-mono text-xs sm:col-start-2 sm:row-start-1"
                value={row.value}
                placeholder={valueLabel}
                aria-label={valueLabel}
                onChange={(event) => updateRow(row.id, { value: event.target.value })}
              />
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                className="col-start-2 row-start-1 text-muted-foreground hover:text-destructive sm:col-start-3"
                aria-label={`${removeLabel}: ${row.key || keyLabel}`}
                title={`${removeLabel}: ${row.key || keyLabel}`}
                onClick={() => onChange(rows.filter((item) => item.id !== row.id))}
              >
                <Trash2 aria-hidden="true" />
              </Button>
            </div>
          ))}
        </div>
      ) : (
        <p className="rounded-lg border border-dashed border-[var(--hairline)] px-3 py-4 text-center text-xs text-muted-foreground">
          {emptyText}
        </p>
      )}
      <Button type="button" size="sm" variant="secondary" className="w-fit" onClick={() => onChange([...rows, createRow()])}>
        <Plus aria-hidden="true" />
        {addLabel}
      </Button>
    </div>
  )
}
