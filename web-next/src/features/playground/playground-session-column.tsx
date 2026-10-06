import { Button, Chip } from '@heroui/react'
import { ArrowDown, MessageSquareText, RefreshCw, Square, TriangleAlert } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
import { PlaygroundMessageItem } from './playground-message'
import { isPlaygroundTranscriptNearBottom } from './playground-scroll'
import { playgroundStatusClassName } from './playground-status'
import type { PlaygroundSession } from './playground-types'

type PlaygroundSessionColumnProps = {
  session: PlaygroundSession
  retryDisabled: boolean
  onRetry: (model: string) => Promise<boolean>
  onStop: (model: string) => void
}

export function PlaygroundSessionColumn({
  session,
  retryDisabled,
  onRetry,
  onStop,
}: PlaygroundSessionColumnProps) {
  const { t } = useTranslation()
  const transcriptRef = useRef<HTMLDivElement>(null)
  const transcriptContentRef = useRef<HTMLDivElement>(null)
  const pinnedToBottomRef = useRef(true)
  const scrollFrameRef = useRef<number | undefined>(undefined)
  const [showScrollToBottom, setShowScrollToBottom] = useState(false)

  const scrollToBottom = useCallback(() => {
    if (!pinnedToBottomRef.current || scrollFrameRef.current !== undefined) return
    scrollFrameRef.current = window.requestAnimationFrame(() => {
      scrollFrameRef.current = undefined
      const transcript = transcriptRef.current
      if (transcript && pinnedToBottomRef.current) {
        transcript.scrollTop = transcript.scrollHeight
      }
    })
  }, [])

  useEffect(() => {
    scrollToBottom()
  }, [scrollToBottom, session.messages])

  useEffect(() => {
    const transcript = transcriptRef.current
    const content = transcriptContentRef.current
    if (!transcript || !content) return

    // Markdown、代码高亮和流式正文会在提交后继续改变高度，观察真实内容尺寸才能稳定跟随。
    const observer = new ResizeObserver(scrollToBottom)
    observer.observe(content)
    observer.observe(transcript)
    return () => {
      observer.disconnect()
      if (scrollFrameRef.current !== undefined) {
        window.cancelAnimationFrame(scrollFrameRef.current)
        scrollFrameRef.current = undefined
      }
    }
  }, [scrollToBottom])

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
        ) : status === 'ready' ? null : (
          <Chip className={playgroundStatusClassName(status)} size="sm" variant="flat">
            {t(`playground.status.${status}`)}
          </Chip>
        )}
        {session.requestState === 'streaming' ? (
          <Button
            isIconOnly
            aria-label={t('playground.actions.stopModel', { model: session.model })}
            className="size-6 min-w-6"
            size="sm"
            title={t('playground.actions.stopModel', { model: session.model })}
            type="button"
            variant="light"
            onClick={() => onStop(session.model)}
          >
            <Square className="size-3" aria-hidden="true" />
          </Button>
        ) : null}
      </header>
      <div className="relative min-h-0">
        <div
          ref={transcriptRef}
          className="h-full min-h-0 overflow-y-auto bg-background/18"
          aria-label={t('playground.comparison.transcript', { model: session.model })}
          onScroll={(event) => {
            const element = event.currentTarget
            const pinned = isPlaygroundTranscriptNearBottom(element)
            pinnedToBottomRef.current = pinned
            setShowScrollToBottom(!pinned)
          }}
        >
          <div ref={transcriptContentRef} className="min-h-full">
            {session.messages.length === 0 ? (
              <div className="ai-workbench-surface grid min-h-full place-items-center overflow-hidden px-6 py-12 text-center">
                <div className="max-w-xs">
                  <div className="mx-auto grid size-10 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1 text-muted-foreground">
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
                {session.requestState === 'interrupted' ? (
                  <div role="status" className="playground-notice-enter mx-3 mb-4 mt-1 flex gap-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2.5 text-xs leading-5 text-warning">
                    <TriangleAlert className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
                    <span>{t('playground.errors.interrupted')}</span>
                  </div>
                ) : session.errorKind ? (
                  <div role="alert" className="playground-notice-enter m-3 flex items-start gap-3 rounded-lg border border-destructive/20 bg-destructive/8 px-3 py-2.5 text-xs text-destructive">
                    <p className="min-w-0 flex-1 leading-5">{t(`playground.errors.${session.errorKind}`)}</p>
                    <Button
                      type="button"
                      size="sm"
                      variant="bordered"
                      className="h-7 shrink-0 border-destructive/25 bg-background/70 px-2 text-destructive hover:bg-destructive/10 hover:text-destructive"
                      isDisabled={retryDisabled}
                      onClick={() => void onRetry(session.model)}
                    >
                      <RefreshCw className="size-3.5" aria-hidden="true" />
                      {t('playground.actions.retry')}
                    </Button>
                  </div>
                ) : null}
              </>
            )}
          </div>
        </div>
        {showScrollToBottom ? (
          <Button
            isIconOnly
            aria-label={t('playground.actions.scrollToBottom')}
            className="playground-notice-enter absolute bottom-3 left-1/2 z-10 -translate-x-1/2 rounded-full bg-surface-1/94 backdrop-blur-xl"
            size="sm"
            title={t('playground.actions.scrollToBottom')}
            type="button"
            variant="bordered"
            onClick={() => {
              const transcript = transcriptRef.current
              if (!transcript) return
              pinnedToBottomRef.current = true
              setShowScrollToBottom(false)
              transcript.scrollTo({ top: transcript.scrollHeight, behavior: 'smooth' })
            }}
          >
            <ArrowDown className="size-3.5" aria-hidden="true" />
          </Button>
        ) : null}
      </div>
    </article>
  )
}
