import { Check, Copy, Terminal as TerminalIcon } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BorderBeam } from '@/components/brand/border-beam'
import { copyText } from '@/lib/clipboard'
import { cn } from '@/lib/utils'

type TerminalTabKey = 'curl' | 'claude' | 'cursor' | 'python' | 'node'

interface TabConfig {
  key: TerminalTabKey
  label: string
  badge?: string
  snippet: string
  rendered: React.ReactNode
}

export function HeroTerminal() {
  const { t } = useTranslation()
  const [activeTab, setActiveTab] = useState<TerminalTabKey>('curl')
  const [copied, setCopied] = useState(false)

  const tabs: TabConfig[] = [
    {
      key: 'curl',
      label: 'cURL',
      snippet: `export OPENAI_BASE_URL="https://api.anyflows.com/v1"
export OPENAI_API_KEY="sk-anyflows-..."

curl $OPENAI_BASE_URL/chat/completions \\
  -H "Authorization: Bearer $OPENAI_API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": "claude-opus-5-5",
    "stream": true,
    "messages": [{"role": "user", "content": "Hello AnyFlows"}]
  }'`,
      rendered: (
        <>
          <div className="text-muted-foreground/75"># 1. 设置统一 Base URL 与 API Key</div>
          <div>
            <span className="text-primary font-medium">export </span>
            <span className="text-foreground">OPENAI_BASE_URL</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"https://api.anyflows.com/v1"</span>
          </div>
          <div>
            <span className="text-primary font-medium">export </span>
            <span className="text-foreground">OPENAI_API_KEY</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"sk-anyflows-..."</span>
          </div>
          <div className="mt-3 text-muted-foreground/75"># 2. 统一接口调用 Claude、GPT、DeepSeek</div>
          <div>
            <span className="text-primary font-medium">curl </span>
            <span className="text-muted-foreground">$OPENAI_BASE_URL/chat/completions \</span>
          </div>
          <div className="pl-4">
            <span className="text-muted-foreground">-H </span>
            <span className="text-success">"Authorization: Bearer $OPENAI_API_KEY" \</span>
          </div>
          <div className="pl-4">
            <span className="text-muted-foreground">-H </span>
            <span className="text-success">"Content-Type: application/json" \</span>
          </div>
          <div className="pl-4">
            <span className="text-muted-foreground">-d </span>
            <span className="text-amber-500/90 dark:text-amber-400">'{`{`}</span>
          </div>
          <div className="pl-8">
            <span className="text-info">"model"</span>
            <span className="text-muted-foreground">: </span>
            <span className="text-success">"claude-opus-5-5"</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div className="pl-8">
            <span className="text-info">"stream"</span>
            <span className="text-muted-foreground">: </span>
            <span className="text-warning">true</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div className="pl-8">
            <span className="text-info">"messages"</span>
            <span className="text-muted-foreground">: [{`{"role": "user", "content": "Hello AnyFlows"}`}]</span>
          </div>
          <div className="pl-4">
            <span className="text-amber-500/90 dark:text-amber-400">{`}'`}</span>
          </div>
        </>
      ),
    },
    {
      key: 'claude',
      label: 'Claude Code',
      badge: 'CLI',
      snippet: `export ANTHROPIC_BASE_URL="https://api.anyflows.com"
export ANTHROPIC_API_KEY="sk-anyflows-..."

# 立即启动官方 Claude Code，无缝调度 Opus 与 Sonnet
claude`,
      rendered: (
        <>
          <div className="text-muted-foreground/75"># 导出 Anthropic 环境变量，原生驱动 Claude Code CLI</div>
          <div>
            <span className="text-primary font-medium">export </span>
            <span className="text-foreground">ANTHROPIC_BASE_URL</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"https://api.anyflows.com"</span>
          </div>
          <div>
            <span className="text-primary font-medium">export </span>
            <span className="text-foreground">ANTHROPIC_API_KEY</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"sk-anyflows-..."</span>
          </div>
          <div className="mt-4 text-muted-foreground/75"># 启动 Claude Code CLI，直接使用 3.7 Sonnet / Opus 矩阵</div>
          <div>
            <span className="text-primary font-semibold">$ claude</span>
          </div>
          <div className="mt-2 text-xs text-muted-foreground/75">
            &gt; Claude Code is ready. Connected to AnyFlows Relay (latency: 82ms).
          </div>
        </>
      ),
    },
    {
      key: 'cursor',
      label: 'Cursor',
      badge: 'IDE',
      snippet: `# Cursor Settings > Models:
# 1. 开启 [Override OpenAI Base URL]
# 2. Base URL 填入: https://api.anyflows.com/v1
# 3. API Key 填入: sk-anyflows-...
# 4. 在 Model List 中添加 claude-opus-5-5 或 gpt-6.1-sol`,
      rendered: (
        <>
          <div className="text-muted-foreground/75"># Cursor 设置 &gt; Models &gt; OpenAI API Key</div>
          <div className="mt-1">
            <span className="text-info font-medium">1. 开启开关: </span>
            <span className="text-foreground">Override OpenAI Base URL</span>
          </div>
          <div>
            <span className="text-info font-medium">2. Base URL: </span>
            <span className="text-success font-semibold">https://api.anyflows.com/v1</span>
          </div>
          <div>
            <span className="text-info font-medium">3. API Key:  </span>
            <span className="text-success">sk-anyflows-****************</span>
          </div>
          <div className="mt-3 text-muted-foreground/75"># 在 Model List 中直接填入模型名称：</div>
          <div className="flex flex-wrap gap-1.5 pt-1">
            <span className="rounded bg-surface-2 px-2 py-0.5 text-xs text-primary font-mono">claude-opus-5-5</span>
            <span className="rounded bg-surface-2 px-2 py-0.5 text-xs text-success font-mono">gpt-6.1-sol</span>
            <span className="rounded bg-surface-2 px-2 py-0.5 text-xs text-info font-mono">deepseek-flash</span>
          </div>
        </>
      ),
    },
    {
      key: 'python',
      label: 'Python',
      snippet: `from openai import OpenAI

client = OpenAI(
    base_url="https://api.anyflows.com/v1",
    api_key="sk-anyflows-...",
)

response = client.chat.completions.create(
    model="claude-opus-5-5",
    messages=[{"role": "user", "content": "Explain quantum computing"}],
    stream=True,
)
for chunk in response:
    print(chunk.choices[0].delta.content or "", end="")`,
      rendered: (
        <>
          <div>
            <span className="text-primary font-medium">from </span>
            <span className="text-foreground">openai </span>
            <span className="text-primary font-medium">import </span>
            <span className="text-foreground">OpenAI</span>
          </div>
          <div className="mt-2">
            <span className="text-foreground">client </span>
            <span className="text-muted-foreground">= </span>
            <span className="text-primary font-medium">OpenAI</span>
            <span className="text-foreground">(</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">base_url</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"https://api.anyflows.com/v1"</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">api_key</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"sk-anyflows-..."</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div><span className="text-foreground">)</span></div>
          <div className="mt-2 text-muted-foreground/75"># 兼容官方 SDK，零代码改动接入</div>
          <div>
            <span className="text-foreground">response </span>
            <span className="text-muted-foreground">= </span>
            <span className="text-foreground">client.chat.completions.create(</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">model</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"claude-opus-5-5"</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">stream</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-warning">True</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">messages</span>
            <span className="text-muted-foreground">=[{`{"role": "user", "content": "AnyFlows"}`}],</span>
          </div>
          <div><span className="text-foreground">)</span></div>
        </>
      ),
    },
    {
      key: 'node',
      label: 'Node.js',
      snippet: `import { createOpenAI } from '@ai-sdk/openai'
import { streamText } from 'ai'

const anyflows = createOpenAI({
  baseURL: 'https://api.anyflows.com/v1',
  apiKey: process.env.ANYFLOWS_API_KEY,
})

const { textStream } = streamText({
  model: anyflows('deepseek-chat'),
  prompt: 'Write a high-performance web app',
})`,
      rendered: (
        <>
          <div>
            <span className="text-primary font-medium">import </span>
            <span className="text-foreground">{`{ createOpenAI }`} </span>
            <span className="text-primary font-medium">from </span>
            <span className="text-success">'@ai-sdk/openai'</span>
          </div>
          <div className="mt-2">
            <span className="text-primary font-medium">const </span>
            <span className="text-foreground">anyflows </span>
            <span className="text-muted-foreground">= </span>
            <span className="text-primary font-medium">createOpenAI</span>
            <span className="text-foreground">({`{`}</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">baseURL</span>
            <span className="text-muted-foreground">: </span>
            <span className="text-success">'https://api.anyflows.com/v1'</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">apiKey</span>
            <span className="text-muted-foreground">: </span>
            <span className="text-foreground">process.env.ANYFLOWS_API_KEY</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div><span className="text-foreground">{`})`}</span></div>
          <div className="mt-2 text-muted-foreground/75"># 原生接入 Vercel AI SDK / LangChain</div>
          <div>
            <span className="text-primary font-medium">const </span>
            <span className="text-foreground">{`{ textStream } `}</span>
            <span className="text-muted-foreground">= </span>
            <span className="text-primary font-medium">streamText</span>
            <span className="text-foreground">({`{`}</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">model</span>
            <span className="text-muted-foreground">: </span>
            <span className="text-foreground">anyflows(</span>
            <span className="text-success">'deepseek-chat'</span>
            <span className="text-foreground">),</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">prompt</span>
            <span className="text-muted-foreground">: </span>
            <span className="text-success">'Write a high-performance app'</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div><span className="text-foreground">{`})`}</span></div>
        </>
      ),
    },
  ]

  const currentTab = tabs.find((t) => t.key === activeTab) ?? tabs[0]

  const handleCopy = async () => {
    const success = await copyText(currentTab.snippet)
    if (success) {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    }
  }

  return (
    <div className="relative group/term w-full">
      {/* Outer ambient glow */}
      <div
        className="pointer-events-none absolute -inset-1 rounded-2xl bg-gradient-to-tr from-primary/20 via-brand/15 to-success/15 blur-xl opacity-70 group-hover/term:opacity-100 transition-opacity duration-700"
        aria-hidden="true"
      />

      <div className="relative rounded-2xl border border-[var(--hairline)] bg-[var(--surface-1)] shadow-2xl backdrop-blur-xl overflow-hidden">
        {/* Animated Border Beam */}
        <BorderBeam radius={16} borderWidth={1.5} beamSize={30} />

        {/* macOS Terminal Title Bar */}
        <div className="flex items-center justify-between border-b border-[var(--hairline)] bg-surface-sunken/80 px-4 py-3">
          <div className="flex items-center gap-2">
            <span className="size-3 rounded-full bg-[#ff5f56]/90 transition-transform hover:scale-110" />
            <span className="size-3 rounded-full bg-[#ffbd2e]/90 transition-transform hover:scale-110" />
            <span className="size-3 rounded-full bg-[#27c93f]/90 transition-transform hover:scale-110" />
            <div className="ml-2 flex items-center gap-1.5 font-mono text-xs font-medium text-muted-foreground">
              <TerminalIcon className="size-3.5 text-primary" />
              <span>anyflows — zsh</span>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <span className="inline-flex items-center gap-1.5 rounded-full border border-success/30 bg-success/10 px-2.5 py-0.5 text-[0.6875rem] font-medium text-success">
              <span className="size-1.5 rounded-full bg-success animate-pulse" />
              <span>Online · 82ms</span>
            </span>
          </div>
        </div>

        {/* Subbar with Language/Client Tabs */}
        <div className="flex items-center justify-between gap-2 border-b border-[var(--hairline)] bg-surface-1/60 px-3 py-1.5 overflow-x-auto scrollbar-none">
          <div className="flex items-center gap-1 shrink-0">
            {tabs.map((tab) => (
              <button
                key={tab.key}
                type="button"
                onClick={() => {
                  setActiveTab(tab.key)
                  setCopied(false)
                }}
                className={cn(
                  'relative rounded-md px-2.5 py-1 text-xs font-medium transition-all duration-200 cursor-pointer',
                  activeTab === tab.key
                    ? 'bg-primary/15 text-primary shadow-xs font-semibold'
                    : 'text-muted-foreground hover:bg-surface-2 hover:text-foreground',
                )}
              >
                <span>{tab.label}</span>
                {tab.badge && (
                  <span className="ml-1.5 rounded bg-primary/20 px-1 py-0.2 text-[0.625rem] text-primary uppercase">
                    {tab.badge}
                  </span>
                )}
              </button>
            ))}
          </div>

          {/* Copy Button */}
          <button
            type="button"
            onClick={handleCopy}
            className="inline-flex items-center gap-1.5 rounded-md border border-[var(--hairline)] bg-surface-2/70 px-2.5 py-1 text-xs font-medium text-muted-foreground transition-all duration-150 hover:bg-surface-2 hover:text-foreground active:scale-95 cursor-pointer shrink-0 ml-2"
            title={copied ? t('landing.home.terminal.copied') : t('landing.home.terminal.copy')}
          >
            {copied ? (
              <>
                <Check className="size-3.5 text-success" />
                <span className="text-success font-medium">{t('landing.home.terminal.copied')}</span>
              </>
            ) : (
              <>
                <Copy className="size-3.5" />
                <span>{t('landing.home.terminal.copy')}</span>
              </>
            )}
          </button>
        </div>

        {/* Terminal Code Display */}
        <div className="p-4 sm:p-5 font-mono text-[0.75rem] sm:text-[0.8125rem] leading-relaxed overflow-x-auto min-h-[260px] bg-surface-sunken/40">
          <div className="space-y-1">{currentTab.rendered}</div>
        </div>

        {/* Bottom Feature Micro-Chips */}
        <div className="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--hairline)] bg-surface-1/70 px-4 py-2.5 text-[0.6875rem] text-muted-foreground">
          <div className="flex flex-wrap items-center gap-3">
            <span className="inline-flex items-center gap-1 text-foreground/80">
              <span className="text-primary font-bold">✓</span> OpenAI / Anthropic 兼容
            </span>
            <span className="inline-flex items-center gap-1 text-foreground/80">
              <span className="text-success font-bold">✓</span> 毫秒级自动降级
            </span>
            <span className="inline-flex items-center gap-1 text-foreground/80">
              <span className="text-info font-bold">✓</span> 流式 SSE 透传
            </span>
          </div>
          <span className="hidden sm:inline font-mono text-[0.625rem] text-muted-foreground/60">
            HTTP/2 · TLS 1.3
          </span>
        </div>
      </div>
    </div>
  )
}
