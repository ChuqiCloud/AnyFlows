import type { ClipboardEvent, KeyboardEvent } from 'react'
import { useState } from 'react'
import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Input } from '@/components/ui/input'

type ChannelModelEditorProps = {
  id: string
  value: string[]
  invalid?: boolean
  onChange: (value: string[]) => void
}

/** 用徽标维护模型集合，并支持批量粘贴和键盘快速录入。 */
export function ChannelModelEditor({ id, value, invalid, onChange }: ChannelModelEditorProps) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState('')

  const append = (raw: string) => {
    const additions = raw
      .split(/[\n,]/)
      .map((item) => item.trim())
      .filter(Boolean)
    if (additions.length === 0) return
    onChange([...new Set([...value, ...additions])])
    setDraft('')
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter' || event.key === ',') {
      event.preventDefault()
      append(draft)
    } else if (event.key === 'Backspace' && draft === '' && value.length > 0) {
      onChange(value.slice(0, -1))
    }
  }

  const handlePaste = (event: ClipboardEvent<HTMLInputElement>) => {
    const pasted = event.clipboardData.getData('text')
    if (!/[\n,]/.test(pasted)) return
    event.preventDefault()
    append(pasted)
  }

  return (
    <div
      className="rounded-lg border border-input bg-[var(--surface-sunken)] p-2 focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50"
      aria-invalid={invalid}
    >
      {value.length > 0 ? (
        <div className="mb-2 flex flex-wrap gap-1.5">
          {value.map((model) => (
            <Badge key={model} className="h-7 max-w-full gap-1 border-transparent bg-info/10 pl-2 text-info">
              <span className="truncate font-mono">{model}</span>
              <button
                type="button"
                className="grid size-5 shrink-0 place-items-center rounded-sm hover:bg-info/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                aria-label={t('channels.actions.removeModel', { model })}
                title={t('channels.actions.removeModel', { model })}
                onClick={() => onChange(value.filter((item) => item !== model))}
              >
                <X className="size-3" aria-hidden="true" />
              </button>
            </Badge>
          ))}
        </div>
      ) : null}
      <Input
        id={id}
        className="h-7 border-0 bg-transparent px-1 font-mono text-xs shadow-none focus-visible:ring-0"
        value={draft}
        placeholder={t('channels.form.modelPlaceholder')}
        disabled={value.length >= 512}
        onBlur={() => append(draft)}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
      />
    </div>
  )
}
