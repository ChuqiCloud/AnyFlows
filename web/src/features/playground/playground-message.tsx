import { Check, CircleUserRound, Copy, Cpu, LoaderCircle } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import type { PlaygroundMessage } from './playground-types'
import { PlaygroundMarkdown } from './playground-markdown'
import { shouldShowEmptyAssistantResponse } from './playground-status'

type PlaygroundMessageItemProps = {
  message: PlaygroundMessage
}

export function PlaygroundMessageItem({ message }: PlaygroundMessageItemProps) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const assistant = message.role === 'assistant'
  const streaming = message.status === 'streaming'

  const copy = async () => {
    if (!message.content) return
    try {
      await navigator.clipboard.writeText(message.content)
      setCopied(true)
      window.setTimeout(() => setCopied(false), 1500)
    } catch {
      setCopied(false)
    }
  }

  return (
    <article className="border-b border-[var(--hairline)] px-3 py-4 last:border-b-0 md:px-4">
      <header className="mb-2 flex items-center gap-2 text-xs">
        <span className="grid size-6 place-items-center rounded-lg bg-surface-2 text-muted-foreground">
          {assistant ? <Cpu className="size-3.5" aria-hidden="true" /> : <CircleUserRound className="size-3.5" aria-hidden="true" />}
        </span>
        <span className="font-medium">{t(assistant ? 'playground.message.assistant' : 'playground.message.you')}</span>
        {message.status === 'cancelled' || message.status === 'error' ? (
          <Badge className={message.status === 'error' ? 'bg-destructive/12 text-destructive' : 'bg-warning/12 text-warning'}>
            {t(`playground.message.${message.status}`)}
          </Badge>
        ) : null}
        {message.content ? (
          <Button
            type="button"
            size="icon-xs"
            variant="ghost"
            className="ml-auto"
            aria-label={t(copied ? 'playground.actions.copied' : 'playground.actions.copyMessage')}
            onClick={copy}
          >
            {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
          </Button>
        ) : null}
      </header>
      {assistant ? (
        message.content ? (
          <div className="min-w-0 break-words text-sm leading-6">
            <PlaygroundMarkdown content={message.content} streaming={streaming} />
          </div>
        ) : streaming ? (
          <div className="flex h-8 items-center gap-2 text-xs text-muted-foreground" role="status">
            <LoaderCircle className="size-3.5 animate-spin" aria-hidden="true" />
            {t('playground.message.generating')}
          </div>
        ) : shouldShowEmptyAssistantResponse(message.status) ? (
          <p className="text-xs text-muted-foreground">{t('playground.message.emptyResponse')}</p>
        ) : null
      ) : (
        <p className="ml-auto max-w-[88%] break-words whitespace-pre-wrap rounded-xl bg-surface-2 px-3 py-2.5 text-sm leading-6">
          {message.content}
        </p>
      )}
    </article>
  )
}
