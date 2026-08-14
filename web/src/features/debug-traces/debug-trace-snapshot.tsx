import { Check, Copy } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

type SnapshotPanelProps = {
  title: string
  value?: unknown
  className?: string
  streamed?: boolean
}

type HeaderEntry = {
  name: string
  value: string
}

/** 将安全快照渲染为可复制、可辨认截断状态的紧凑诊断面板。 */
export function DebugTraceSnapshotPanel({ title, value, className, streamed = false }: SnapshotPanelProps) {
  const { t } = useTranslation()
  const [copied, setCopied] = useState(false)
  const resetTimer = useRef<ReturnType<typeof setTimeout>>(undefined)
  const snapshot = useMemo(() => normalizeSnapshot(value), [value])

  useEffect(() => () => {
    if (resetTimer.current) clearTimeout(resetTimer.current)
  }, [])

  const copy = async () => {
    if (!snapshot) return
    try {
      await navigator.clipboard.writeText(snapshot.copyText)
      setCopied(true)
      if (resetTimer.current) clearTimeout(resetTimer.current)
      resetTimer.current = setTimeout(() => setCopied(false), 1_500)
    } catch {
      setCopied(false)
    }
  }

  return (
    <section className={cn('min-w-0 border border-[var(--hairline)] bg-surface-1/45', className)}>
      <header className="flex h-9 items-center justify-between gap-2 border-b border-[var(--hairline)] px-3">
        <h5 className="truncate text-[0.6875rem] font-semibold text-muted-foreground">{title}</h5>
        {snapshot ? (
          <Button type="button" size="icon-xs" variant="ghost" title={t('debugTraces.actions.copy')} aria-label={t('debugTraces.actions.copy')} onClick={() => void copy()}>
            {copied ? <Check className="text-success" aria-hidden="true" /> : <Copy aria-hidden="true" />}
          </Button>
        ) : null}
      </header>
      {snapshot?.headers ? (
        <dl className="max-h-64 overflow-auto py-1 font-mono text-[0.6875rem]">
          {snapshot.headers.map((entry, index) => (
            <div key={`${entry.name}-${index}`} className="grid grid-cols-[minmax(7rem,0.36fr)_minmax(0,1fr)] gap-3 px-3 py-1.5 odd:bg-surface-2/35">
              <dt className="break-all text-muted-foreground">{entry.name}</dt>
              <dd className="break-all text-foreground">{entry.value}</dd>
            </div>
          ))}
        </dl>
      ) : snapshot ? (
        <div>
          <pre className="max-h-72 overflow-auto whitespace-pre-wrap break-words px-3 py-2.5 font-mono text-[0.6875rem] leading-5 text-foreground">{snapshot.displayText}</pre>
          {snapshot.truncated ? <p className="border-t border-warning/20 bg-warning/8 px-3 py-1.5 text-[0.6875rem] text-warning">{t('debugTraces.detail.truncated', { bytes: snapshot.originalBytes })}</p> : null}
        </div>
      ) : (
        <p className="px-3 py-4 text-xs text-muted-foreground">{t(streamed ? 'debugTraces.detail.streamNotBuffered' : 'debugTraces.detail.notCaptured')}</p>
      )}
    </section>
  )
}

function normalizeSnapshot(value: unknown) {
  const headers = headerEntries(value)
  if (headers) {
    return {
      headers,
      displayText: '',
      copyText: headers.map(({ name, value }) => `${name}: ${value}`).join('\n'),
      truncated: false,
      originalBytes: 0,
    }
  }
  if (isRecord(value) && typeof value.content === 'string') {
    return {
      headers: undefined,
      displayText: value.content,
      copyText: value.content,
      truncated: value.truncated === true,
      originalBytes: typeof value.original_bytes === 'number' ? value.original_bytes : 0,
    }
  }
  if (value === undefined || value === null) return undefined
  const text = JSON.stringify(value, null, 2)
  return {
    headers: undefined,
    displayText: text,
    copyText: text,
    truncated: false,
    originalBytes: 0,
  }
}

function headerEntries(value: unknown): HeaderEntry[] | undefined {
  if (!Array.isArray(value)) return undefined
  const entries = value.filter((entry): entry is HeaderEntry => (
    isRecord(entry) && typeof entry.name === 'string' && typeof entry.value === 'string'
  ))
  return entries.length === value.length ? entries : undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
