import type { ClipboardEvent, KeyboardEvent } from 'react'
import { useEffect, useState } from 'react'
import { Plus, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { isValidIpEntry, isValidIpList } from './api-key-form-model'

type ApiKeyIpEditorProps = {
  values: string[]
  error?: string
  onChange: (values: string[]) => void
}

/** 逐条维护 IP/CIDR，避免向用户暴露 JSON 或多行文本格式。 */
export function ApiKeyIpEditor({ values, error, onChange }: ApiKeyIpEditorProps) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState('')
  const [inputError, setInputError] = useState<string>()

  useEffect(() => setInputError(undefined), [values])

  const append = (candidates: string[]) => {
    const normalized = candidates.map((value) => value.trim()).filter(Boolean)
    if (normalized.length === 0) return
    if (normalized.some((value) => !isValidIpEntry(value))) {
      setInputError(t('apiKeys.ipEditor.invalid'))
      return
    }
    const next = [...new Set([...values, ...normalized])]
    if (!isValidIpList(next)) {
      setInputError(t('apiKeys.ipEditor.limit'))
      return
    }
    onChange(next)
    setDraft('')
    setInputError(undefined)
  }

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key !== 'Enter' && event.key !== ',') return
    event.preventDefault()
    append([draft])
  }

  const onPaste = (event: ClipboardEvent<HTMLInputElement>) => {
    const entries = event.clipboardData.getData('text').split(/[\s,]+/)
    if (entries.filter(Boolean).length <= 1) return
    event.preventDefault()
    append(entries)
  }

  return (
    <div className="grid gap-2">
      <div className="flex gap-2">
        <Input
          id="api-key-ips"
          className="min-w-0 font-mono text-xs"
          value={draft}
          aria-invalid={!!error || !!inputError}
          placeholder={t('apiKeys.ipEditor.placeholder')}
          onChange={(event) => { setDraft(event.target.value); setInputError(undefined) }}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
        />
        <Button
          type="button"
          size="icon"
          variant="secondary"
          aria-label={t('apiKeys.ipEditor.add')}
          disabled={!draft.trim()}
          onClick={() => append([draft])}
        >
          <Plus aria-hidden="true" />
        </Button>
      </div>
      {values.length > 0 ? (
        <div className="flex max-h-24 flex-wrap gap-1.5 overflow-y-auto" aria-label={t('apiKeys.ipEditor.selected')}>
          {values.map((value) => (
            <Badge key={value} className="max-w-full gap-1 bg-surface-2 pr-0.5 font-mono text-muted-foreground">
              <span className="truncate">{value}</span>
              <button
                type="button"
                className="grid size-4 shrink-0 place-items-center rounded-sm hover:bg-background focus-visible:ring-2 focus-visible:ring-ring/60"
                aria-label={t('apiKeys.ipEditor.remove', { value })}
                onClick={() => onChange(values.filter((entry) => entry !== value))}
              >
                <X className="size-3" aria-hidden="true" />
              </button>
            </Badge>
          ))}
        </div>
      ) : null}
      {inputError ? <p role="alert" className="text-xs text-destructive">{inputError}</p> : null}
    </div>
  )
}
