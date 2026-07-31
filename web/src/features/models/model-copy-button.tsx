import { Check, Copy } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'

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
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size={size}
          className="shrink-0 text-muted-foreground hover:text-foreground"
          aria-label={label}
          onClick={() => void copy()}
        >
          <Icon className="size-3.5" aria-hidden="true" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
