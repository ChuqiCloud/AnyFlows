import { GitFork, KeyRound } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type { CredentialFormMode } from './credential-form-model'

/** 创建时先选择普通根凭据或固定形状的 Spark 影子。 */
export function CredentialModePicker({ value, disabled, sparkDisabled, onChange }: {
  value: CredentialFormMode
  disabled?: boolean
  sparkDisabled?: boolean
  onChange: (mode: CredentialFormMode) => void
}) {
  const { t } = useTranslation()
  const modes = [
    { value: 'standard', icon: KeyRound },
    { value: 'spark_shadow', icon: GitFork },
  ] as const

  return (
    <div className="grid gap-1 rounded-lg bg-surface-2 p-1 sm:grid-cols-2" role="radiogroup" aria-label={t('credentials.form.mode')}>
      {modes.map((mode) => {
        const Icon = mode.icon
        return (
          <Button
            key={mode.value}
            type="button"
            size="sm"
            variant="ghost"
            role="radio"
            aria-checked={value === mode.value}
            disabled={disabled || (mode.value === 'spark_shadow' && sparkDisabled)}
            className={cn('h-auto min-h-11 justify-start px-3 py-2 text-left', value === mode.value && 'bg-background text-foreground shadow-xs hover:bg-background')}
            onClick={() => onChange(mode.value)}
          >
            <Icon className="size-4 shrink-0" aria-hidden="true" />
            <span className="min-w-0">
              <span className="block text-xs font-medium">{t(`credentials.form.modeLabel.${mode.value}`)}</span>
              <span className="mt-0.5 block text-[0.6875rem] leading-4 text-muted-foreground">{t(`credentials.form.modeHint.${mode.value}`)}</span>
            </span>
          </Button>
        )
      })}
    </div>
  )
}
