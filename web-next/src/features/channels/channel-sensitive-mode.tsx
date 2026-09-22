import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import type { SensitiveEditMode } from './channel-form-model'

type ChannelSensitiveModeProps = {
  id: string
  value: SensitiveEditMode
  allowPreserve: boolean
  onChange: (value: SensitiveEditMode) => void
}

/** 显式选择敏感对象的保留、替换或清空语义。 */
export function ChannelSensitiveMode({ id, value, allowPreserve, onChange }: ChannelSensitiveModeProps) {
  const { t } = useTranslation()
  const modes: SensitiveEditMode[] = allowPreserve
    ? ['preserve', 'replace', 'clear']
    : ['replace', 'clear']

  return (
    <div
      id={id}
      role="radiogroup"
      aria-label={t('channels.sensitiveMode.label')}
      className={cn('grid gap-1 rounded-lg bg-surface-2 p-1', allowPreserve ? 'grid-cols-3' : 'grid-cols-2')}
    >
      {modes.map((mode) => (
        <button
          key={mode}
          type="button"
          role="radio"
          aria-checked={value === mode}
          className={cn(
            'h-7 rounded-md px-2 text-xs font-medium text-muted-foreground outline-none transition-colors',
            'hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring',
            value === mode && 'bg-surface-1 text-foreground',
          )}
          onClick={() => onChange(mode)}
        >
          {t(`channels.sensitiveMode.${mode}`)}
        </button>
      ))}
    </div>
  )
}
