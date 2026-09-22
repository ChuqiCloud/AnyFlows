import { Button, Chip, Input } from '@heroui/react'
import { useState, type ClipboardEvent, type KeyboardEvent } from 'react'
import { Plus } from 'lucide-react'
import { useTranslation } from 'react-i18next'

type ModelTagEditorProps = {
  id: string
  tags: string[]
  disabled?: boolean
  onChange: (tags: string[]) => void
}

/** 用逐项输入和徽标维护有界标签集合，不向管理员暴露 JSON。 */
export function ModelTagEditor({ id, tags, disabled, onChange }: ModelTagEditorProps) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState('')

  const commit = (values: string[]) => {
    const next = [...tags]
    for (const value of values) {
      const normalized = value.trim()
      if (normalized && !next.includes(normalized) && next.length < 32) next.push(normalized)
    }
    onChange(next)
    setDraft('')
  }

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter' || event.key === ',') {
      event.preventDefault()
      commit([draft])
    } else if (event.key === 'Backspace' && !draft && tags.length > 0) {
      onChange(tags.slice(0, -1))
    }
  }

  const onPaste = (event: ClipboardEvent<HTMLInputElement>) => {
    const text = event.clipboardData.getData('text')
    if (!/[,\n]/.test(text)) return
    event.preventDefault()
    commit(text.split(/[,\n]/))
  }

  return (
    <div className="grid gap-2">
      <div className="flex gap-2">
        <Input
          id={id}
          size="sm"
          value={draft}
          isDisabled={disabled || tags.length >= 32}
          placeholder={t('modelManagement.tags.placeholder')}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
        />
        <Button isIconOnly type="button" size="md" variant="bordered" isDisabled={disabled || !draft.trim() || tags.length >= 32} aria-label={t('modelManagement.tags.add')} onClick={() => commit([draft])}>
          <Plus className="size-4" aria-hidden="true" />
        </Button>
      </div>
      {tags.length > 0 ? (
        <div className="flex min-h-8 flex-wrap gap-1.5" aria-label={t('modelManagement.tags.selected')}>
          {tags.map((tag) => (
            <Chip
              key={tag}
              classNames={{
                base: 'h-7 max-w-full shrink-0 !flex-nowrap pl-2',
                content: 'min-w-0 truncate whitespace-nowrap',
                closeButton: 'inline-flex size-5 shrink-0 self-center items-center justify-center [&>svg]:block',
              }}
              isDisabled={disabled}
              size="sm"
              variant="flat"
              onClose={() => onChange(tags.filter((item) => item !== tag))}
            >
              {tag}
            </Chip>
          ))}
        </div>
      ) : <p className="text-xs text-muted-foreground">{t('modelManagement.tags.empty')}</p>}
    </div>
  )
}
