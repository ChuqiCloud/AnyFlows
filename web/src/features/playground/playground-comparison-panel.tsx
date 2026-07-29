import { MessageSquareText, SendHorizontal, Square } from 'lucide-react'
import {
  type CSSProperties,
  type FormEvent,
  type KeyboardEvent,
  useEffect,
  useRef,
  useState,
} from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { cn } from '@/lib/utils'
import { PlaygroundSessionColumn } from './playground-session-column'
import type { PlaygroundDisplayStatus } from './playground-status'
import type { PlaygroundSession } from './playground-types'

type PlaygroundComparisonPanelProps = {
  apiKeyReady: boolean
  canSend: boolean
  draft: string
  sessions: PlaygroundSession[]
  status: PlaygroundDisplayStatus
  streaming: boolean
  onDraftChange: (value: string) => void
  onSend: () => Promise<boolean>
  onStopAll: () => void
  onStopModel: (model: string) => void
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
    <section className="grid min-h-[34rem] min-w-0 grid-rows-[minmax(0,1fr)_auto] overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1">
      {columnCount === 0 ? (
        <div className="grid min-h-0 place-items-center px-6 py-12 text-center">
          <div className="max-w-sm">
            <div className="mx-auto grid size-10 place-items-center rounded-xl bg-surface-2 text-muted-foreground">
              <MessageSquareText className="size-4" aria-hidden="true" />
            </div>
            <h3 className="mt-3 text-sm font-semibold">{t('playground.model.pending')}</h3>
            <p className="mt-1 text-xs leading-5 text-muted-foreground">
              {t('playground.model.empty')}
            </p>
          </div>
        </div>
      ) : (
        <div className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] xl:grid-rows-1">
          {columnCount > 1 ? (
            <div
              className="flex min-w-0 overflow-x-auto border-b border-[var(--hairline)] xl:hidden"
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
                      'min-w-32 flex-1 truncate border-b-2 border-transparent px-3 py-2 font-mono text-xs text-muted-foreground outline-none',
                      'hover:bg-surface-2/60 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60',
                      active && 'border-primary bg-surface-2/60 text-foreground',
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
              className="grid h-full grid-cols-1 gap-px bg-[var(--hairline)] xl:min-w-[var(--comparison-track-min)] xl:[grid-template-columns:repeat(var(--comparison-columns),minmax(var(--comparison-column-min),1fr))]"
              style={trackStyle}
            >
              {props.sessions.map((session) => (
                <div
                  key={session.model}
                  className={cn('min-h-0 min-w-0', session.model !== visibleModel && 'hidden xl:block')}
                  role={columnCount > 1 ? 'tabpanel' : undefined}
                >
                  <PlaygroundSessionColumn session={session} onStop={props.onStopModel} />
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      <form className="border-t border-[var(--hairline)] p-2.5" onSubmit={submit}>
        <Textarea
          ref={composerRef}
          value={props.draft}
          className="max-h-40 min-h-20 resize-y border-0 bg-[var(--surface-sunken)] text-sm leading-6 shadow-none focus-visible:ring-2"
          maxLength={100_000}
          placeholder={t(props.apiKeyReady
            ? 'playground.composer.placeholder'
            : 'playground.composer.apiKeyRequired')}
          disabled={props.streaming}
          aria-label={t('playground.composer.label')}
          onChange={(event) => props.onDraftChange(event.target.value)}
          onKeyDown={composerKeyDown}
        />
        <div className="mt-2 flex min-h-8 flex-wrap items-center justify-between gap-2">
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-[0.6875rem] text-muted-foreground" aria-live="polite">
            <span>{t(`playground.status.${props.status}`)}</span>
            {columnCount > 0 ? (
              <span className="tabular-nums">{t('playground.comparison.summary', { count: columnCount })}</span>
            ) : null}
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
      </form>
    </section>
  )
}
