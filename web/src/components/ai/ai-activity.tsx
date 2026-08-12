import type { ReactNode } from 'react'

import { cn } from '@/lib/utils'

type AiActivityProps = {
  active?: boolean
  className?: string
  detail?: ReactNode
  label: ReactNode
  size?: 'compact' | 'default' | 'panel'
  tone?: 'brand' | 'muted' | 'success' | 'warning' | 'destructive'
}

const toneClassNames = {
  brand: 'text-info',
  muted: 'text-muted-foreground',
  success: 'text-success',
  warning: 'text-warning',
  destructive: 'text-destructive',
} as const

/** 统一表达流式生成、工具执行和后台任务，不承载任何业务状态机。 */
export function AiActivity({
  active = true,
  className,
  detail,
  label,
  size = 'default',
  tone = 'brand',
}: AiActivityProps) {
  const compact = size === 'compact'
  const panel = size === 'panel'

  return (
    <div
      className={cn(
        'flex min-w-0 items-center',
        compact ? 'gap-2' : 'gap-2.5',
        panel && 'rounded-xl border border-[var(--hairline)] bg-surface-1/84 px-3.5 py-3 shadow-subtle backdrop-blur-xl',
        className,
      )}
      data-ai-active={active}
      role={active ? 'status' : undefined}
    >
      <span
        className={cn(
          'grid shrink-0 grid-cols-3 text-current',
          compact ? 'gap-px' : 'gap-[1.5px]',
          toneClassNames[tone],
        )}
        aria-hidden="true"
      >
        {Array.from({ length: 9 }, (_, index) => (
          <span
            key={index}
            className={cn(
              'ai-activity-pixel rounded-[1px] bg-current',
              compact ? 'size-[3px]' : 'size-1',
            )}
          />
        ))}
      </span>
      <span className="min-w-0">
        <span
          className={cn(
            'ai-activity-label block truncate font-medium',
            compact ? 'text-[0.6875rem]' : 'text-xs',
          )}
          data-ai-active={active}
        >
          {label}
        </span>
        {detail ? (
          <span className={cn('mt-0.5 block truncate text-muted-foreground', compact ? 'text-[0.625rem]' : 'text-[0.6875rem]')}>
            {detail}
          </span>
        ) : null}
      </span>
    </div>
  )
}
