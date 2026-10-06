import type { ReactNode } from 'react'
import type { LucideIcon } from 'lucide-react'

import { cn } from '@/lib/utils'

export function CredentialOAuthNotice({ children, icon: Icon, message, spinning = false, tone }: {
  children?: ReactNode
  icon?: LucideIcon
  message: string
  spinning?: boolean
  tone: 'error' | 'info' | 'neutral' | 'success'
}) {
  const tones = {
    error: 'border-destructive/25 bg-destructive/8 text-destructive',
    info: 'border-info/20 bg-info/8 text-info',
    neutral: 'border-[var(--hairline)] bg-background text-muted-foreground',
    success: 'border-success/20 bg-success/8 text-success',
  } as const
  return (
    <div role={tone === 'error' ? 'alert' : tone === 'success' ? 'status' : undefined} className={cn('flex min-h-9 items-center gap-2 rounded-lg border px-3 py-2 text-xs', tones[tone])}>
      {Icon ? <Icon className={cn('size-3.5 shrink-0', spinning && 'animate-spin')} aria-hidden="true" /> : null}
      <span className="min-w-0 flex-1 leading-4">{message}</span>
      {children}
    </div>
  )
}
