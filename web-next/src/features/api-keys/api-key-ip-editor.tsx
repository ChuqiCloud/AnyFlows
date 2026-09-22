import type { ClipboardEvent, KeyboardEvent } from 'react'
import { useEffect, useState } from 'react'
import { Button, Chip, Input } from '@heroui/react'
import { Plus } from 'lucide-react'
import { useTranslation } from 'react-i18next'

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
          className="min-w-0"
          classNames={{ input: 'font-mono text-xs' }}
          id="api-key-ips"
          isInvalid={!!error || !!inputError}
          placeholder={t('apiKeys.ipEditor.placeholder')}
          size="sm"
          value={draft}
          onChange={(event) => { setDraft(event.target.value); setInputError(undefined) }}
          onKeyDown={onKeyDown}
          onPaste={onPaste}
        />
        <Button
          isIconOnly
          aria-label={t('apiKeys.ipEditor.add')}
          isDisabled={!draft.trim()}
          size="md"
          type="button"
          variant="bordered"
          onClick={() => append([draft])}
        >
          <Plus className="size-4" aria-hidden="true" />
        </Button>
      </div>
      {values.length > 0 ? (
        <div className="flex min-w-0 gap-1.5 overflow-hidden" aria-label={t('apiKeys.ipEditor.selected')}>
          {values.map((value) => (
            <Chip
              key={value}
              classNames={{
                base: 'max-w-40 shrink-0 !flex-nowrap bg-surface-2 font-mono text-muted-foreground',
                content: 'min-w-0 truncate whitespace-nowrap',
                closeButton: 'inline-flex size-4 shrink-0 self-center items-center justify-center [&>svg]:block',
              }}
              size="sm"
              variant="flat"
              onClose={() => onChange(values.filter((entry) => entry !== value))}
            >
              {value}
            </Chip>
          ))}
        </div>
      ) : null}
      {inputError ? <p role="alert" className="text-xs text-destructive">{inputError}</p> : null}
    </div>
  )
}
