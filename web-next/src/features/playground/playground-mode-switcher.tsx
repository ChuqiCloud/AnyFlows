import { MessageSquareText, Video } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'

export type PlaygroundMode = 'chat' | 'video'

type PlaygroundModeSwitcherProps = {
  disabled?: boolean
  mode: PlaygroundMode
  onChange: (mode: PlaygroundMode) => void
}

/** 在同一试炼场入口切换对话与视频工作区，不参与各模式的请求状态管理。 */
export function PlaygroundModeSwitcher(props: PlaygroundModeSwitcherProps) {
  const { t } = useTranslation()

  return (
    <div
      className="relative grid h-9 grid-cols-2 items-center rounded-lg border border-[var(--hairline)] bg-surface-2/55 p-0.5"
      role="tablist"
      aria-label={t('playground.modes.label')}
    >
      <span
        className="pointer-events-none absolute inset-y-0.5 left-0.5 w-[calc(50%-0.125rem)] rounded-md bg-surface-1 transition-transform duration-200 ease-[var(--ease-ai-out)]"
        data-ai-motion="move"
        style={{ transform: props.mode === 'video' ? 'translateX(100%)' : 'translateX(0)' }}
        aria-hidden="true"
      />
      <button
        type="button"
        role="tab"
        aria-selected={props.mode === 'chat'}
        disabled={props.disabled}
        className={cn(
          'relative z-10 flex h-8 items-center justify-center gap-1.5 rounded-md px-2.5 text-xs font-medium outline-none transition-colors duration-150',
          'focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40',
          props.mode === 'chat' ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
        )}
        onClick={() => props.onChange('chat')}
      >
        <MessageSquareText className="size-3.5" aria-hidden="true" />
        {t('playground.modes.chat')}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={props.mode === 'video'}
        disabled={props.disabled}
        className={cn(
          'relative z-10 flex h-8 items-center justify-center gap-1.5 rounded-md px-2.5 text-xs font-medium outline-none transition-colors duration-150',
          'focus-visible:ring-2 focus-visible:ring-ring/60 disabled:cursor-not-allowed disabled:opacity-40',
          props.mode === 'video' ? 'text-foreground' : 'text-muted-foreground hover:text-foreground',
        )}
        onClick={() => props.onChange('video')}
      >
        <Video className="size-3.5" aria-hidden="true" />
        {t('playground.modes.video')}
      </button>
    </div>
  )
}
