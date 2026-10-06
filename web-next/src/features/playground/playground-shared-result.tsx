import { Chip } from '@heroui/react'
import { LockKeyhole } from 'lucide-react'
import { type CSSProperties, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import type { PlaygroundShareSession } from '@/lib/api/generated/types.gen'
import { PlaygroundMessageItem } from './playground-message'

type PlaygroundSharedResultProps = {
  sessions: PlaygroundShareSession[]
}

export function PlaygroundSharedResult({ sessions }: PlaygroundSharedResultProps) {
  const { t } = useTranslation()
  const [activeModel, setActiveModel] = useState(sessions[0]?.model ?? '')

  useEffect(() => {
    if (!sessions.some((session) => session.model === activeModel)) {
      setActiveModel(sessions[0]?.model ?? '')
    }
  }, [activeModel, sessions])

  const visibleModel = sessions.some((session) => session.model === activeModel)
    ? activeModel
    : sessions[0]?.model ?? ''
  const columnCount = sessions.length
  const trackStyle = {
    '--comparison-columns': Math.max(columnCount, 1),
    '--comparison-column-min': columnCount > 2 ? '22rem' : '0rem',
    '--comparison-track-min': columnCount > 2 ? `${columnCount * 22}rem` : '100%',
  } as CSSProperties

  return (
    <section className="grid min-h-[32rem] min-w-0 overflow-hidden rounded-xl border border-[var(--hairline)] bg-surface-1">
      <div className="grid min-h-0 grid-rows-[auto_minmax(0,1fr)] xl:grid-rows-1">
        {columnCount > 1 ? (
          <div
            className="flex min-w-0 overflow-x-auto border-b border-[var(--hairline)] xl:hidden"
            role="tablist"
            aria-label={t('playground.share.public.modelTabs')}
          >
            {sessions.map((session) => {
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
            {sessions.map((session, sessionIndex) => (
              <article
                key={session.model}
                className={cn(
                  'grid min-h-0 min-w-0 grid-rows-[3.25rem_minmax(0,1fr)] bg-surface-1',
                  session.model !== visibleModel && 'hidden xl:grid',
                )}
                role={columnCount > 1 ? 'tabpanel' : undefined}
              >
                <header className="flex min-w-0 items-center gap-2 border-b border-[var(--hairline)] px-3">
                  <p className="min-w-0 flex-1 truncate font-mono text-xs font-medium" title={session.model}>
                    {session.model}
                  </p>
                  <Chip className="bg-info/12 text-info" size="sm" variant="flat">
                    <LockKeyhole className="size-3.5" aria-hidden="true" />{t('playground.share.public.readOnly')}
                  </Chip>
                </header>
                <div className="min-h-0 overflow-y-auto" aria-label={t('playground.share.public.transcript', { model: session.model })}>
                  {session.messages.map((message, messageIndex) => (
                    <PlaygroundMessageItem
                      key={`${sessionIndex}-${messageIndex}`}
                      message={{
                        id: `${sessionIndex}-${messageIndex}`,
                        content: message.content,
                        role: message.role,
                        status: 'complete',
                      }}
                    />
                  ))}
                </div>
              </article>
            ))}
          </div>
        </div>
      </div>
    </section>
  )
}
