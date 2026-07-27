import * as React from 'react'
import { ChevronDown } from 'lucide-react'

import { cn } from '@/lib/utils'

/** 保留原生键盘与移动端选择体验的 shadcn 风格单选控件。 */
function Select({ className, children, ...props }: React.ComponentProps<'select'>) {
  return (
    <div className="relative">
      <select
        data-slot="select"
        className={cn(
          'h-8 w-full appearance-none rounded-lg border border-input bg-transparent px-2.5 pr-8 text-sm outline-none transition-colors',
          'focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50',
          className,
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDown className="pointer-events-none absolute right-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
    </div>
  )
}

export { Select }
