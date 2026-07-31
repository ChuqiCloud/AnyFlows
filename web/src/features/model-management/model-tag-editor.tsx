import { useState, type ClipboardEvent, type KeyboardEvent } from 'react'
import { Plus, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'

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
          value={draft}
          disabled={disabled || tags.length >= 32}
          placeholder={t('modelManagement.tags.placeholder')}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
        />
        <Button type="button" size="icon" variant="secondary" disabled={disabled || !draft.trim() || tags.length >= 32} aria-label={t('modelManagement.tags.add')} onClick={() => commit([draft])}>
          <Plus aria-hidden="true" />
        </Button>
      </div>
      {tags.length > 0 ? (
        <div className="flex min-h-8 flex-wrap gap-1.5" aria-label={t('modelManagement.tags.selected')}>
          {tags.map((tag) => (
            <Badge key={tag} className="h-7 gap-1.5 pl-2 pr-1">
              <span>{tag}</span>
              <button type="button" disabled={disabled} className="grid size-5 place-items-center rounded-sm text-muted-foreground hover:bg-background/60 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none" aria-label={t('modelManagement.tags.remove', { tag })} onClick={() => onChange(tags.filter((item) => item !== tag))}>
                <X className="size-3" aria-hidden="true" />
              </button>
            </Badge>
          ))}
        </div>
      ) : <p className="text-xs text-muted-foreground">{t('modelManagement.tags.empty')}</p>}
    </div>
  )
}
