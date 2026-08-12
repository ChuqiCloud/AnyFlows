import { MessageSquareText, Square } from 'lucide-react'
import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { PlaygroundMessageItem } from './playground-message'
import { playgroundStatusClassName } from './playground-status'
import type { PlaygroundSession } from './playground-types'

type PlaygroundSessionColumnProps = {
  session: PlaygroundSession
  onStop: (model: string) => void
}

export function PlaygroundSessionColumn({ session, onStop }: PlaygroundSessionColumnProps) {
  const { t } = useTranslation()
  const transcriptRef = useRef<HTMLDivElement>(null)
  const pinnedToBottomRef = useRef(true)

  useEffect(() => {
    const transcript = transcriptRef.current
    if (transcript && pinnedToBottomRef.current) transcript.scrollTop = transcript.scrollHeight
  }, [session.messages])

  const status = session.requestState === 'idle' ? 'ready' : session.requestState

  return (
    <article className="grid h-full min-h-0 grid-rows-[3.75rem_minmax(0,1fr)] bg-surface-1">
      <header className="flex min-w-0 items-center gap-2 border-b border-dashed border-[var(--hairline)] bg-surface-1/90 px-3.5">
        <div className="min-w-0 flex-1">
          <p className="truncate font-mono text-xs font-medium" title={session.model}>{session.model}</p>
          <p className="mt-0.5 truncate text-[0.625rem] text-muted-foreground tabular-nums">
            {session.usage
              ? t('playground.usage.summary', {
                  input: session.usage.inputTokens ?? 0,
                  output: session.usage.outputTokens ?? 0,
                })
              : t('playground.usage.pending')}
          </p>
        </div>
        {session.requestState === 'streaming' ? (
          <AiActivity active label={t('playground.status.streaming')} size="compact" />
        ) : (
          <Badge className={playgroundStatusClassName(status)}>
            {t(`playground.status.${status}`)}
          </Badge>
        )}
        {session.requestState === 'streaming' ? (
          <Button
            type="button"
            size="icon-xs"
            variant="ghost"
            title={t('playground.actions.stopModel', { model: session.model })}
            aria-label={t('playground.actions.stopModel', { model: session.model })}
            onClick={() => onStop(session.model)}
          >
            <Square aria-hidden="true" />
          </Button>
        ) : null}
      </header>
      <div
        ref={transcriptRef}
        className="min-h-0 overflow-y-auto bg-background/18"
        aria-label={t('playground.comparison.transcript', { model: session.model })}
        onScroll={(event) => {
          const element = event.currentTarget
          pinnedToBottomRef.current = element.scrollHeight - element.scrollTop - element.clientHeight < 96
        }}
      >
        {session.messages.length === 0 ? (
          <div className="ai-workbench-surface grid min-h-full place-items-center overflow-hidden px-6 py-12 text-center">
            <div className="max-w-xs">
              <div className="mx-auto grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1 text-muted-foreground shadow-subtle">
                <MessageSquareText className="size-4" aria-hidden="true" />
              </div>
              <h3 className="mt-3 text-sm font-semibold">{t('playground.empty.title')}</h3>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{t('playground.empty.body')}</p>
            </div>
          </div>
        ) : (
          <>
            {session.messages.map((message) => (
              <PlaygroundMessageItem key={message.id} message={message} />
            ))}
            {session.errorKind ? (
              <p role="alert" className="m-3 rounded-lg border border-destructive/20 bg-destructive/8 px-2.5 py-2 text-xs text-destructive">
                {t(`playground.errors.${session.errorKind}`)}
              </p>
            ) : null}
          </>
        )}
      </div>
    </article>
  )
}
