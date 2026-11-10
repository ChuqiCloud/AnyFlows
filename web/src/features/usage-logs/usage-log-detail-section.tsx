import { useState, type MouseEvent } from 'react'
import { Check, Copy, type LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'

export type DetailItem = {
  label: string
  value: string
  mono?: boolean
  wide?: boolean
  action?: () => void
  copyable?: boolean
  valueClassName?: string
}

function DetailCopy({ value }: { value: string }) {
  const { t } = useTranslation()
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle')
  const label = t(state === 'copied' ? 'usageLogs.actions.copiedValue' : state === 'failed' ? 'usageLogs.actions.copyValueFailed' : 'usageLogs.actions.copyValue')

  const copy = async (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation()
    try {
      await navigator.clipboard.writeText(value)
      setState('copied')
    } catch {
      setState('failed')
    }
  }

  return (
    <button type="button" className={cn('inline-flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground outline-none hover:bg-surface-2 hover:text-foreground focus-visible:ring-2 focus-visible:ring-brand', state === 'failed' && 'text-destructive')} aria-label={label} title={label} onClick={(event) => void copy(event)}>
      {state === 'copied' ? <Check className="size-3.5 text-success" aria-hidden="true" /> : <Copy className="size-3.5" aria-hidden="true" />}
      <span className="sr-only" aria-live="polite">{state === 'idle' ? '' : label}</span>
    </button>
  )
}

export function DetailSection({ title, icon: Icon, items }: { title: string; icon: LucideIcon; items: DetailItem[] }) {
  return (
    <section className="border-b border-[var(--hairline)] py-5 last:border-b-0">
      <h3 className="flex items-center gap-2 text-xs font-semibold">
        <Icon className="size-3.5 text-muted-foreground" aria-hidden="true" />
        {title}
      </h3>
      <dl className="mt-3 grid grid-cols-2 gap-x-6 gap-y-4">
        {items.map((item) => (
          <div key={item.label} className={cn('min-w-0', item.wide && 'col-span-2')}>
            <dt className="text-[0.6875rem] text-muted-foreground">{item.label}</dt>
            <dd className={cn('mt-1 flex items-start gap-1 text-sm tabular-nums', item.mono && 'font-mono text-xs', item.valueClassName)}>
              <span className="min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]">
                {item.action ? <button type="button" className="text-left font-medium text-brand outline-none hover:underline focus-visible:underline" onClick={item.action}>{item.value}</button> : item.value}
              </span>
              {item.copyable ? <DetailCopy key={item.value} value={item.value} /> : null}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  )
}
