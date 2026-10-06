import type { ComponentProps } from 'react'

import { cn } from '@/lib/utils'

type KbdProps = ComponentProps<'kbd'> & {
  keys: readonly string[]
}

/** 扁平键帽：细边 + 表面色，不做渐变浮雕。 */
export function Kbd({ keys, className, ...props }: KbdProps) {
  return (
    <kbd className={cn('inline-flex items-center gap-1 font-sans', className)} {...props}>
      {keys.map((key) => (
        <span
          key={key}
          className="grid h-5 min-w-5 place-items-center rounded-md border border-[var(--hairline)] bg-surface-2 px-1.5 text-[0.6875rem] font-medium text-muted-foreground"
        >
          {key}
        </span>
      ))}
    </kbd>
  )
}
