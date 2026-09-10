import { MessageSquareText, SendHorizontal, Square } from 'lucide-react'
import {
  type CSSProperties,
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from 'react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/lib/utils'
import { PlaygroundSessionColumn } from './playground-session-column'
import type { PlaygroundDisplayStatus } from './playground-status'
import type { PlaygroundSession } from './playground-types'

type PlaygroundComparisonPanelProps = {
  canSend: boolean
  draft: string
  modelPicker: ReactNode
  sessions: PlaygroundSession[]
  status: PlaygroundDisplayStatus
  streaming: boolean
  onDraftChange: (value: string) => void
  onRetry: (model: string) => Promise<boolean>
  onSend: () => Promise<boolean>
  onStopAll: () => void
  onStopModel: (model: string) => void
}

/** 模型替换时沿用首条消息标识，避免同一对话列被 React 重新挂载。 */
function playgroundSessionRenderKey(session: PlaygroundSession) {
  return session.messages[0]?.id ?? `empty:${session.model}`
}

export function PlaygroundComparisonPanel(props: PlaygroundComparisonPanelProps) {
  const { t } = useTranslation()
  const [activeModel, setActiveModel] = useState(props.sessions[0]?.model ?? '')
  const composerRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (!props.sessions.some((session) => session.model === activeModel)) {
      setActiveModel(props.sessions[0]?.model ?? '')
    }
  }, [activeModel, props.sessions])

  const visibleModel = props.sessions.some((session) => session.model === activeModel)
    ? activeModel
    : props.sessions[0]?.model ?? ''

  const submit = (event?: FormEvent) => {
    event?.preventDefault()
    if (!props.canSend) return
    void props.onSend().finally(() => composerRef.current?.focus())
  }

  const composerKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault()
      submit()
    }
  }

  const columnCount = props.sessions.length
  const trackStyle = {
    '--comparison-columns': Math.max(columnCount, 1),
    '--comparison-column-min': columnCount > 2 ? '22rem' : '0rem',
    '--comparison-track-min': columnCount > 2 ? `${columnCount * 22}rem` : '100%',
  } as CSSProperties

  return (
    <section className="grid h-full min-h-[36rem] min-w-0 grid-rows-[minmax(0,1fr)_auto] overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 shadow-sm transition-[border-color,box-shadow] duration-200 ease-[var(--ease-ai-out)]">
      {columnCount === 0 ? (
        <div className="ai-workbench-surface grid min-h-0 place-items-center overflow-hidden px-6 py-12 text-center">
          <div className="max-w-sm">
            <div className="mx-auto grid size-11 place-items-center rounded-xl border border-[var(--hairline)] bg-surface-1 text-info shadow-md">
              <MessageSquareText className="size-4.5" aria-hidden="true" />
            </div>
            <h3 className="mt-4 text-sm font-semibold">{t('playground.model.pending')}</h3>
            <p className="mt-1.5 text-xs leading-5 text-muted-foreground">
              {t('playground.model.empty')}
            </p>
          </div>
        </div>
      ) : (
        <div className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] xl:grid-rows-1">
          {columnCount > 1 ? (
            <div
              className="flex min-w-0 gap-1 overflow-x-auto border-b border-[var(--hairline)] bg-surface-2/35 p-1 xl:hidden"
              role="tablist"
              aria-label={t('playground.comparison.modelTabs')}
            >
              {props.sessions.map((session) => {
                const active = session.model === visibleModel
                return (
                  <button
                    key={session.model}
                    type="button"
                    role="tab"
                    aria-selected={active}
                    className={cn(
                      'min-w-32 flex-1 truncate rounded-lg px-3 py-2 font-mono text-xs text-muted-foreground outline-none transition-[background-color,color,box-shadow] duration-150',
                      'hover:bg-surface-2/70 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60',
                      active && 'bg-surface-1 text-foreground shadow-subtle',
                    )}
                    onClick={() => setActiveModel(session.model)}
                  >
                    {session.model}
                  </button>
                )
              })}
            </div>
          ) : null}
          <div className="min-h-0 overflow-x-auto">
            <div
              className="grid h-full grid-cols-1 xl:min-w-[var(--comparison-track-min)] xl:[grid-template-columns:repeat(var(--comparison-columns),minmax(var(--comparison-column-min),1fr))]"
              style={trackStyle}
            >
              {props.sessions.map((session, index) => (
                <div
                  key={playgroundSessionRenderKey(session)}
                  className={cn(
                    'min-h-0 min-w-0',
                    index > 0 && 'xl:border-l xl:border-dashed xl:border-[var(--hairline)]',
                    session.model !== visibleModel && 'hidden xl:block',
                  )}
                  role={columnCount > 1 ? 'tabpanel' : undefined}
                >
                  <PlaygroundSessionColumn
                    session={session}
                    retryDisabled={props.streaming}
                    onRetry={props.onRetry}
                    onStop={props.onStopModel}
                  />
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      <form className="bg-background/45 px-3 pb-3 pt-2 backdrop-blur-xl md:px-5 md:pb-4" onSubmit={submit}>
        <div className="mx-auto max-w-4xl overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1 shadow-md transition-[border-color,box-shadow,transform] duration-200 ease-[var(--ease-ai-out)] focus-within:-translate-y-0.5 focus-within:border-info/40 focus-within:shadow-[var(--shadow-glow-info)]">
          <Textarea
            ref={composerRef}
            value={props.draft}
            className="max-h-44 min-h-20 resize-y rounded-none border-0 bg-transparent px-4 py-3.5 text-sm leading-6 shadow-none focus-visible:ring-0"
            maxLength={100_000}
            placeholder={t('playground.composer.placeholder')}
            disabled={props.streaming}
            aria-label={t('playground.composer.label')}
            onChange={(event) => props.onDraftChange(event.target.value)}
            onKeyDown={composerKeyDown}
          />
          <div className="flex min-h-11 flex-wrap items-center justify-between gap-2 border-t border-dashed border-[var(--hairline)] px-2.5 py-1.5">
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[0.6875rem] text-muted-foreground" aria-live="polite">
              {props.streaming ? (
                <AiActivity active label={t(`playground.status.${props.status}`)} size="compact" />
              ) : (
                <span>{t(`playground.status.${props.status}`)}</span>
              )}
              {columnCount > 0 ? (
                <span className="tabular-nums">
                  {t(
                    columnCount === 1
                      ? 'playground.comparison.singleSummary'
                      : 'playground.comparison.summary',
                    { count: columnCount },
                  )}
                </span>
              ) : null}
            </div>
            <div className="flex min-w-0 items-center gap-2">
              <div className="min-w-0 max-w-[min(100%,20rem)] flex-1 md:max-w-[24rem]">
                {props.modelPicker}
              </div>
              {props.streaming ? (
                <Button type="button" size="sm" variant="secondary" onClick={props.onStopAll}>
                  <Square aria-hidden="true" />{t('playground.actions.stopAll')}
                </Button>
              ) : (
                <Button type="submit" size="sm" disabled={!props.canSend}>
                  <SendHorizontal aria-hidden="true" />
                  {columnCount > 1
                    ? t('playground.actions.sendMultiple', { count: columnCount })
                    : t('playground.actions.send')}
                </Button>
              )}
            </div>
          </div>
        </div>
      </form>
    </section>
  )
}
