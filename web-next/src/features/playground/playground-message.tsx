import { Button, Chip } from '@heroui/react'
import { Check, CircleUserRound, Copy, Cpu } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AiActivity } from '@/components/ai/ai-activity'
import { cn } from '@/lib/utils'
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
    <article className={assistant ? 'ai-message-enter group px-3 py-3.5 md:px-5' : 'group px-3 py-3.5 md:px-5'}>
      <div className={assistant ? 'mx-auto max-w-4xl' : 'mx-auto flex max-w-4xl justify-end'}>
        <div className={assistant ? 'w-full' : 'max-w-[88%]'}>
          <header className={assistant ? 'mb-2 flex items-center gap-2 text-xs' : 'mb-1.5 flex items-center justify-end gap-2 text-xs'}>
            <span className="grid size-6 place-items-center rounded-lg border border-[var(--hairline)] bg-surface-1 text-muted-foreground">
              {assistant ? <Cpu className="size-3.5" aria-hidden="true" /> : <CircleUserRound className="size-3.5" aria-hidden="true" />}
            </span>
            <span className="font-medium">{t(assistant ? 'playground.message.assistant' : 'playground.message.you')}</span>
            {message.status === 'cancelled' || message.status === 'interrupted' || message.status === 'error' ? (
              <Chip className={message.status === 'error' ? 'bg-destructive/12 text-destructive' : 'bg-warning/12 text-warning'} size="sm" variant="flat">
                {t(`playground.message.${message.status}`)}
              </Chip>
            ) : null}
            {message.content ? (
              <Button
                isIconOnly
                aria-label={t(copied ? 'playground.actions.copied' : 'playground.actions.copyMessage')}
                className={cn('size-6 min-w-6 opacity-60 transition-opacity group-hover:opacity-100', assistant && 'ml-auto')}
                size="sm"
                type="button"
                variant="light"
                onClick={copy}
              >
                {copied ? <Check className="size-3" aria-hidden="true" /> : <Copy className="size-3" aria-hidden="true" />}
              </Button>
            ) : null}
          </header>
          {assistant ? (
            message.content ? (
              <div className={streaming ? 'ai-streaming-copy min-w-0 break-words text-sm leading-6' : 'min-w-0 break-words text-sm leading-6'}>
                <PlaygroundMarkdown content={message.content} streaming={streaming} />
              </div>
            ) : streaming ? (
              <AiActivity active detail={t('playground.status.streaming')} label={t('playground.message.generating')} />
            ) : shouldShowEmptyAssistantResponse(message.status) ? (
              <p className="text-xs text-muted-foreground">{t('playground.message.emptyResponse')}</p>
            ) : null
          ) : (
            <p className="break-words whitespace-pre-wrap rounded-xl border border-[var(--hairline)] bg-surface-2/78 px-3 py-2.5 text-sm leading-6">
              {message.content}
            </p>
          )}
        </div>
      </div>
    </article>
  )
}
