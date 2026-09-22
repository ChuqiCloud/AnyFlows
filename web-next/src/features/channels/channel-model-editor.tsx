import type { ClipboardEvent, KeyboardEvent } from 'react'
import { useState } from 'react'
import { Chip, Input } from '@heroui/react'
import { useTranslation } from 'react-i18next'

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
            <Chip
              key={model}
              classNames={{
                base: 'h-7 max-w-full shrink-0 !flex-nowrap bg-info/10 pl-2 text-info',
                content: 'min-w-0 truncate whitespace-nowrap font-mono',
                closeButton: 'inline-flex size-5 shrink-0 self-center items-center justify-center [&>svg]:block',
              }}
              size="sm"
              variant="flat"
              onClose={() => onChange(value.filter((item) => item !== model))}
            >
              {model}
            </Chip>
          ))}
        </div>
      ) : null}
      {/* 输入区无边框，外层容器已经承担边框与聚焦态。 */}
      <Input
        classNames={{
          base: 'shadow-none',
          inputWrapper: 'h-7 min-h-7 border-0 bg-transparent px-1 shadow-none data-[hover=true]:bg-transparent group-data-[focus=true]:bg-transparent',
          input: 'font-mono text-xs',
        }}
        id={id}
        isDisabled={value.length >= 512}
        placeholder={t('channels.form.modelPlaceholder')}
        value={draft}
        onBlur={() => append(draft)}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
      />
    </div>
  )
}
