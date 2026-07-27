import * as React from 'react'

import { cn } from '@/lib/utils'

function Badge({ className, ...props }: React.ComponentProps<'span'>) {
  return (
    <span
      data-slot="badge"
      className={cn(
        'inline-flex h-5 items-center rounded-sm border border-[var(--hairline)] bg-surface-2 px-1.5 text-[0.6875rem] font-medium leading-none',
        className,
      )}
      {...props}
    />
  )
}

export { Badge }
