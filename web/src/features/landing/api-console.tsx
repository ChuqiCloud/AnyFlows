import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'

/**
 * 三种协议各一份请求/响应样例。
 *
 * 刻意用同一个 endpoint 前缀（/v1）与同一枚 key 占位：这里要证明的正是
 * "同一个网关、同一把凭据，换协议只换报文形状"，而不是罗列三套接入文档。
 */
type ProtocolSample = {
  id: 'openai' | 'openai-responses' | 'anthropic' | 'gemini'
  label: string
  endpoint: string
  /** 请求头差异是协议识别的关键，逐条列出而不合并 */
  headers: readonly string[]
  request: readonly string[]
  response: readonly string[]
  latencyMs: number
  tokensIn: number
  tokensOut: number
}

const samples: readonly ProtocolSample[] = [
  {
    id: 'openai',
    label: 'OpenAI',
    endpoint: '/v1/chat/completions',
    headers: ['Authorization: Bearer af-••••'],
    request: [
      '{',
      '  "model": "gpt-5.6-sol",',
      '  "messages": [{ "role": "user", "content": "ping" }],',
      '  "stream": true',
      '}',
    ],
    response: [
      '{',
      '  "choices": [{ "message": { "content": "pong" } }],',
      '  "usage": { "prompt_tokens": 9, "completion_tokens": 4 }',
      '}',
    ],
    latencyMs: 138,
    tokensIn: 9,
    tokensOut: 4,
  },
  {
    id: 'openai-responses',
    label: 'Responses',
    endpoint: '/v1/responses',
    headers: ['Authorization: Bearer af-••••'],
    request: [
      '{',
      '  "model": "gpt-5.6-sol",',
      '  "input": "ping"',
      '}',
    ],
    response: [
      '{',
      '  "output": [{',
      '    "type": "message",',
      '    "content": [{ "type": "output_text", "text": "pong" }]',
      '  }],',
      '  "usage": { "input_tokens": 9, "output_tokens": 4 }',
      '}',
    ],
    latencyMs: 144,
    tokensIn: 9,
    tokensOut: 4,
  },
  {
    id: 'anthropic',
    label: 'Anthropic',
    endpoint: '/v1/messages',
    headers: ['x-api-key: af-••••', 'anthropic-version: 2023-06-01'],
    request: [
      '{',
      '  "model": "claude-opus-5",',
      '  "max_tokens": 1024,',
      '  "messages": [{ "role": "user", "content": "ping" }]',
      '}',
    ],
    response: [
      '{',
      '  "content": [{ "type": "text", "text": "pong" }],',
      '  "usage": { "input_tokens": 9, "output_tokens": 4 }',
      '}',
    ],
    latencyMs: 152,
    tokensIn: 9,
    tokensOut: 4,
  },
  {
    id: 'gemini',
    label: 'Gemini',
    endpoint: '/v1beta/models/{model}:generateContent',
    headers: ['x-goog-api-key: af-••••'],
    request: [
      '{',
      '  "contents": [',
      '    { "role": "user", "parts": [{ "text": "ping" }] }',
      '  ]',
      '}',
    ],
    response: [
      '{',
      '  "candidates": [{ "content": { "parts": [{ "text": "pong" }] } }],',
      '  "usageMetadata": { "totalTokenCount": 13 }',
      '}',
    ],
    latencyMs: 121,
    tokensIn: 9,
    tokensOut: 4,
  },
] as const

/** 自动轮播间隔。比 new-api 的 4.5s 略长——我们的报文更长，读完需要更多时间 */
const cycleMs = 5200

export function ApiConsole() {
  const { t } = useTranslation()
  const [activeId, setActiveId] = useState<ProtocolSample['id']>('openai')
  /** 用户点过 tab 后就停止轮播：自动切换会打断正在读的人 */
  const [pinned, setPinned] = useState(false)
  const tabRefs = useRef<Partial<Record<ProtocolSample['id'], HTMLButtonElement | null>>>({})

  useEffect(() => {
    if (pinned) {
      return
    }

    // 尊重减弱动效：静态停在首个协议上，不自动轮播
    const root = document.documentElement
    if (root.dataset.motion !== 'full') {
      return
    }

    const timer = window.setInterval(() => {
      setActiveId((current) => {
        const index = samples.findIndex((sample) => sample.id === current)
        return samples[(index + 1) % samples.length].id
      })
    }, cycleMs)

    return () => window.clearInterval(timer)
  }, [pinned])

  const active = samples.find((sample) => sample.id === activeId) ?? samples[0]

  /** 左右方向键在 tab 间移动，符合 WAI-ARIA tabs 模式 */
  const handleKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') {
      return
    }

    event.preventDefault()
    const index = samples.findIndex((sample) => sample.id === activeId)
    const offset = event.key === 'ArrowRight' ? 1 : -1
    const next = samples[(index + offset + samples.length) % samples.length]

    setPinned(true)
    setActiveId(next.id)
    tabRefs.current[next.id]?.focus()
  }

  return (
    <div className="mt-16 overflow-hidden rounded-xl border border-[var(--hairline)] bg-[var(--surface-sunken)] shadow-[var(--shadow-lg)]">
      <div
        role="tablist"
        aria-label={t('landing.console.tablist')}
        className="flex items-center gap-1 border-b border-[var(--hairline)] px-2 sm:px-3"
      >
        {samples.map((sample) => {
          const isActive = sample.id === active.id

          return (
            <button
              key={sample.id}
              ref={(node) => {
                tabRefs.current[sample.id] = node
              }}
              role="tab"
              id={`api-tab-${sample.id}`}
              aria-selected={isActive}
              aria-controls={`api-panel-${sample.id}`}
              tabIndex={isActive ? 0 : -1}
              onClick={() => {
                setPinned(true)
                setActiveId(sample.id)
              }}
              onKeyDown={handleKeyDown}
              className={cn(
                '-mb-px border-b-2 px-3 py-3 text-[0.8125rem] font-medium transition-colors',
                'focus-visible:ring-2 focus-visible:ring-brand/50 focus-visible:outline-none',
                isActive
                  ? 'border-brand text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground',
              )}
            >
              {sample.label}
            </button>
          )
        })}

        <span className="ml-auto flex items-center gap-2 pr-2 sm:pr-3">
          <span
            className="size-1.5 rounded-full bg-[var(--success)] shadow-[0_0_8px_var(--success)]"
            aria-hidden="true"
          />
          <span className="font-mono text-[0.625rem] tracking-normal text-muted-foreground uppercase">
            200 ok
          </span>
        </span>
      </div>

      {samples.map((sample) => (
        <div
          key={sample.id}
          role="tabpanel"
          id={`api-panel-${sample.id}`}
          aria-labelledby={`api-tab-${sample.id}`}
          hidden={sample.id !== active.id}
        >
          <SamplePanel sample={sample} />
        </div>
      ))}
    </div>
  )
}

function SamplePanel({ sample }: { sample: ProtocolSample }) {
  const { t } = useTranslation()

  return (
    <>
      <div className="flex items-center gap-2.5 border-b border-[var(--hairline)] px-5 py-3">
        <span className="rounded-md bg-brand/10 px-1.5 py-0.5 font-mono text-[0.625rem] font-semibold tracking-normal text-brand">
          POST
        </span>
        <code className="truncate font-mono text-xs text-foreground/80">{sample.endpoint}</code>
      </div>

      {/*
        两块等高栅格：切换协议时报文行数不同，若任其自适应，
        整段会随 tab 抽动一次，把注意力从内容拽到布局上。
      */}
      <div className="grid md:grid-cols-2 md:divide-x md:divide-[var(--hairline)]">
        <CodeBlock label={t('landing.console.request')}>
          <span className="text-muted-foreground">$ curl -X POST</span>{' '}
          <span className="text-foreground/80">{sample.endpoint}</span>
          {sample.headers.map((header) => (
            <div key={header} className="text-muted-foreground">
              {'    -H '}
              <span className="text-foreground/70">&quot;{header}&quot;</span>
            </div>
          ))}
          <div className="mt-2 text-foreground/80">{sample.request.join('\n')}</div>
        </CodeBlock>

        <CodeBlock label={t('landing.console.response')}>
          <div className="text-foreground/80">{sample.response.join('\n')}</div>
        </CodeBlock>
      </div>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 border-t border-[var(--hairline)] px-5 py-3 font-mono text-[0.6875rem] text-muted-foreground tabular-nums">
        <Metric label={t('landing.console.latency')} value={`${sample.latencyMs}ms`} />
        <Metric label={t('landing.console.tokens')} value={`${sample.tokensIn}/${sample.tokensOut}`} />
        <Metric label={t('landing.console.route')} value={t('landing.protocol.canonical')} />
      </div>
    </>
  )
}

function CodeBlock({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="px-5 py-4">
      <p className="text-[0.625rem] font-semibold tracking-normal text-muted-foreground/70 uppercase">
        {label}
      </p>
      {/*
        min-h 让请求块与响应块在三个协议间保持一致高度。
        whitespace-pre-wrap 保留 JSON 缩进，同时允许窄屏折行。
      */}
      <pre className="mt-3 min-h-[9.5rem] overflow-x-auto font-mono text-[0.75rem] leading-[1.6] whitespace-pre-wrap">
        {children}
      </pre>
    </div>
  )
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span className="tracking-normal uppercase opacity-60">{label}</span>
      <span className="text-foreground/70">{value}</span>
    </span>
  )
}
