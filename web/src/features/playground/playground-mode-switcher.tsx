import { MessageSquareText, Video } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'

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
      className="flex items-center rounded-lg border border-[var(--hairline)] bg-surface-2/45 p-0.5"
      role="tablist"
      aria-label={t('playground.modes.label')}
    >
      <Button
        type="button"
        size="sm"
        variant={props.mode === 'chat' ? 'secondary' : 'ghost'}
        role="tab"
        aria-selected={props.mode === 'chat'}
        disabled={props.disabled}
        onClick={() => props.onChange('chat')}
      >
        <MessageSquareText aria-hidden="true" />
        {t('playground.modes.chat')}
      </Button>
      <Button
        type="button"
        size="sm"
        variant={props.mode === 'video' ? 'secondary' : 'ghost'}
        role="tab"
        aria-selected={props.mode === 'video'}
        disabled={props.disabled}
        onClick={() => props.onChange('video')}
      >
        <Video aria-hidden="true" />
        {t('playground.modes.video')}
      </Button>
    </div>
  )
}
