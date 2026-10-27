import Anthropic from '@lobehub/icons/es/Anthropic/components/Mono'
import DeepSeek from '@lobehub/icons/es/DeepSeek/components/Mono'
import Gemini from '@lobehub/icons/es/Gemini/components/Mono'
import OpenAI from '@lobehub/icons/es/OpenAI/components/Mono'
import Qwen from '@lobehub/icons/es/Qwen/components/Mono'
import type { ComponentType } from 'react'

interface TickerItem {
  name: string
  latency: string
  availability: string
  Icon: ComponentType<{ className?: string }>
}

const tickerItems: TickerItem[] = [
  { name: 'Claude 3.7 Sonnet', latency: '82ms', availability: '99.98%', Icon: Anthropic },
  { name: 'OpenAI GPT-4o', latency: '74ms', availability: '99.99%', Icon: OpenAI },
  { name: 'DeepSeek R1', latency: '92ms', availability: '99.95%', Icon: DeepSeek },
  { name: 'Gemini 2.0 Flash', latency: '58ms', availability: '99.99%', Icon: Gemini },
  { name: 'Qwen 2.5 72B', latency: '68ms', availability: '99.95%', Icon: Qwen },
  { name: 'Claude 3.5 Haiku', latency: '48ms', availability: '99.98%', Icon: Anthropic },
  { name: 'DeepSeek V3', latency: '76ms', availability: '99.97%', Icon: DeepSeek },
  { name: 'OpenAI o3-mini', latency: '88ms', availability: '99.96%', Icon: OpenAI },
]

export function HeroTicker() {
  const duplicated = [...tickerItems, ...tickerItems]

  return (
    <div
      className="relative w-full border-y border-[var(--hairline)] bg-surface-1/75 backdrop-blur-md overflow-hidden h-10.5 flex items-center"
      aria-label="实时模型状态监控"
    >
      <div className="absolute left-0 top-0 bottom-0 w-12 z-10 bg-gradient-to-r from-background to-transparent pointer-events-none" />
      <div className="absolute right-0 top-0 bottom-0 w-12 z-10 bg-gradient-to-l from-background to-transparent pointer-events-none" />

      <div
        className="group/ticker flex w-max shrink-0 animate-marquee items-center group-hover/ticker:[animation-play-state:paused]"
        style={{ animationDuration: '42s' }}
      >
        {duplicated.map((item, index) => {
          const Icon = item.Icon

          return (
            <div
              key={`${item.name}-${index}`}
              className="inline-flex items-center gap-2.5 px-6 border-r border-[var(--hairline)] text-xs text-muted-foreground whitespace-nowrap cursor-default"
            >
              <Icon className="size-3.5 text-foreground/80 shrink-0" />
              <span className="font-medium text-foreground/90">{item.name}</span>
              <span className="inline-flex items-center gap-1 font-mono text-[0.6875rem] text-success">
                <span className="size-1.5 rounded-full bg-success animate-pulse" />
                {item.latency}
              </span>
              <span className="font-mono text-[0.625rem] text-muted-foreground/70">
                {item.availability}
              </span>
            </div>
          )
        })}
      </div>
    </div>
  )
}
