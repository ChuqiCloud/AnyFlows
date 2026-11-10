import Anthropic from '@lobehub/icons/es/Anthropic/components/Mono'
import DeepSeek from '@lobehub/icons/es/DeepSeek/components/Mono'
import Gemini from '@lobehub/icons/es/Gemini/components/Mono'
import Kimi from '@lobehub/icons/es/Kimi/components/Mono'
import Minimax from '@lobehub/icons/es/Minimax/components/Mono'
import OpenAI from '@lobehub/icons/es/OpenAI/components/Mono'
import Qwen from '@lobehub/icons/es/Qwen/components/Mono'
import Zhipu from '@lobehub/icons/es/Zhipu/components/Mono'
import {
  AppWindow,
  Bot,
  Box,
  Code2,
  Cpu,
  Monitor,
  Sparkles,
  Terminal,
} from 'lucide-react'
import type { ComponentType } from 'react'

interface EcosystemItem {
  name: string
  category: 'model' | 'tool' | 'os'
  badge: string
  Icon: ComponentType<{ className?: string }>
}

const ecosystemItems: EcosystemItem[] = [
  { name: 'Claude 3.7 / Sonnet', category: 'model', badge: 'Model', Icon: Anthropic },
  { name: 'OpenAI GPT-4o', category: 'model', badge: 'Model', Icon: OpenAI },
  { name: 'DeepSeek R1 & V3', category: 'model', badge: 'Model', Icon: DeepSeek },
  { name: 'Gemini 2.0 Flash', category: 'model', badge: 'Model', Icon: Gemini },
  { name: 'Qwen 2.5', category: 'model', badge: 'Model', Icon: Qwen },
  { name: 'Kimi Moonshot', category: 'model', badge: 'Model', Icon: Kimi },
  { name: 'Zhipu GLM-4', category: 'model', badge: 'Model', Icon: Zhipu },
  { name: 'MiniMax', category: 'model', badge: 'Model', Icon: Minimax },
  { name: 'Claude Code', category: 'tool', badge: 'CLI Agent', Icon: Terminal },
  { name: 'Cursor IDE', category: 'tool', badge: 'Editor', Icon: Sparkles },
  { name: 'Cline / Roo Code', category: 'tool', badge: 'Extension', Icon: Bot },
  { name: 'VS Code', category: 'tool', badge: 'Editor', Icon: Code2 },
  { name: 'JetBrains', category: 'tool', badge: 'IDE', Icon: Box },
  { name: 'OpenCode / NextChat', category: 'tool', badge: 'Web / App', Icon: AppWindow },
  { name: 'macOS', category: 'os', badge: 'Platform', Icon: Monitor },
  { name: 'Linux', category: 'os', badge: 'Platform', Icon: Cpu },
  { name: 'Windows', category: 'os', badge: 'Platform', Icon: Monitor },
]

export function EcosystemStrip() {
  const duplicated = [...ecosystemItems, ...ecosystemItems]

  return (
    <div className="relative border-y border-[var(--hairline)] bg-surface-1/60 py-6 overflow-hidden">
      {/* Label and Subtitle */}
      <div className="mx-auto mb-4 max-w-[1240px] px-4 text-center">
        <p className="text-[0.6875rem] font-semibold tracking-[0.2em] text-muted-foreground/80 uppercase">
          可用模型 · 可用系统 · 开发工具与 Agent 生态
        </p>
      </div>

      {/* Marquee Track */}
      <div
        className="group/eco relative flex overflow-hidden [mask-image:linear-gradient(to_right,transparent,black_10%,black_90%,transparent)]"
        aria-hidden="true"
      >
        <div
          className="flex w-max shrink-0 animate-marquee items-center gap-3.5 group-hover/eco:[animation-play-state:paused]"
          style={{ animationDuration: '65s' }}
        >
          {duplicated.map((item, index) => {
            const Icon = item.Icon

            return (
              <div
                key={`${item.name}-${index}`}
                className="inline-flex items-center gap-2.5 whitespace-nowrap rounded-full border border-[var(--hairline)] bg-surface-1 px-3.5 py-1.5 text-xs text-muted-foreground shadow-xs transition-colors duration-200 hover:border-primary/40 hover:bg-surface-2 hover:text-foreground shrink-0 cursor-default"
              >
                <Icon className="size-4 shrink-0 text-foreground/80" />
                <span className="font-medium text-foreground/90">{item.name}</span>
                <span className="whitespace-nowrap rounded-full bg-surface-2 px-1.5 py-0.2 text-[0.625rem] text-muted-foreground/80">
                  {item.badge}
                </span>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}
