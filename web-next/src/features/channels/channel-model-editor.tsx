import type { ClipboardEvent, KeyboardEvent } from 'react'
import { Button, Chip, Input } from '@heroui/react'
import { useTranslation } from 'react-i18next'

type ChannelModelEditorProps = {
  id: string
  value: string[]
  draft: string
  onDraftChange: (value: string) => void
  invalid?: boolean
  disabled?: boolean
  onChange: (value: string[]) => void
}

export function ChannelModelEditor({ id, value, draft, onDraftChange, invalid, disabled, onChange }: ChannelModelEditorProps) {
  const { t } = useTranslation()

  const append = (raw: string) => {
    const additions = raw
      .split(/[\n,]/)
      .map((item) => item.trim())
      .filter(Boolean)
    if (disabled || additions.length === 0) return
    onChange([...new Set([...value, ...additions])])
    onDraftChange('')
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return
    if (event.key === 'Enter' || event.key === ',') {
      event.preventDefault()
      append(draft)
    }
  }

  const handlePaste = (event: ClipboardEvent<HTMLInputElement>) => {
    const pasted = event.clipboardData.getData('text')
    if (!/[\n,]/.test(pasted)) return
    event.preventDefault()
    append(pasted)
  }

  return (
    <div className="grid min-w-0 gap-2">
      <div className="flex items-start gap-2">
        <Input id={id} size="sm" aria-label={t('channelModels.modelInput.label')} placeholder={t('channelModels.modelInput.placeholder')}
          aria-describedby={`${id}-hint`} isDisabled={disabled} isInvalid={invalid}
          value={draft} onValueChange={onDraftChange} onKeyDown={handleKeyDown} onPaste={handlePaste} />
        <Button type="button" size="sm" variant="bordered" className="shrink-0"
          isDisabled={disabled || value.length >= 512 || !draft.trim()} onPress={() => append(draft)}>{t('channelModels.actions.add')}</Button>
      </div>
      <p id={`${id}-hint`} className="text-xs text-muted-foreground">{t('channelModels.modelInput.hint')}</p>
      {value.length > 0 ? (
        <div className="flex max-h-48 flex-wrap content-start gap-1.5 overflow-y-auto">
          {value.map((model) => (
            <Chip
              key={model}
              classNames={{
                base: 'h-7 max-w-full shrink-0 !flex-nowrap',
                content: 'min-w-0 truncate whitespace-nowrap font-mono',
                closeButton: 'inline-flex size-5 shrink-0 self-center items-center justify-center [&>svg]:block',
              }}
              size="sm"
              title={model}
              variant="flat"
              onClose={disabled ? undefined : () => onChange(value.filter((item) => item !== model))}
            >
              {model}
            </Chip>
          ))}
        </div>
      ) : null}
    </div>
  )
}
