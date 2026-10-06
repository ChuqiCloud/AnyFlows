import { Button } from '@heroui/react'
import { Check, Copy } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { SiteTooltip } from '@/shared/components/site-tooltip'

type ModelCopyButtonProps = {
  model: string
  size?: 'icon-xs' | 'icon-sm'
}

export function ModelCopyButton({ model, size = 'icon-sm' }: ModelCopyButtonProps) {
  const { t } = useTranslation()
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle')

  useEffect(() => {
    if (state === 'idle') return
    const timer = window.setTimeout(() => setState('idle'), 1_600)
    return () => window.clearTimeout(timer)
  }, [state])

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(model)
      setState('copied')
    } catch {
      setState('failed')
    }
  }

  const label = t(state === 'copied' ? 'models.actions.copied' : state === 'failed' ? 'models.actions.copyFailed' : 'models.actions.copy')
  const Icon = state === 'copied' ? Check : Copy
  return (
    <SiteTooltip content={label}>
      <Button
        isIconOnly
        aria-label={label}
        className={size === 'icon-xs' ? 'size-6 min-w-6 shrink-0 text-muted-foreground hover:text-foreground' : 'shrink-0 text-muted-foreground hover:text-foreground'}
        size="sm"
        type="button"
        variant="light"
        onClick={() => void copy()}
      >
        <Icon className="size-3.5" aria-hidden="true" />
      </Button>
    </SiteTooltip>
  )
}
