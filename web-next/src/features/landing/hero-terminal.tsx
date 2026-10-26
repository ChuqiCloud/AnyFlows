import { Check, Copy, Terminal as TerminalIcon } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { BorderBeam } from '@/components/brand/border-beam'
import { copyText } from '@/lib/clipboard'
import { cn } from '@/lib/utils'
import { useLandingBaseUrl, useLandingModels } from './use-landing-models'

type TerminalTabKey = 'curl' | 'claude' | 'cursor' | 'python' | 'node'

interface TabConfig {
  key: TerminalTabKey
  label: string
  badge?: string
  snippet: string
  rendered: ReactNode
}

export function HeroTerminal() {
  const { t } = useTranslation()
  const baseUrl = useLandingBaseUrl()
  const { models } = useLandingModels()
  const sampleModel = models.find((model) => model.output_modalities.includes('text'))?.model ?? 'YOUR_MODEL_ID'
  const openAiBaseUrl = `${baseUrl}/v1`
  const [activeTab, setActiveTab] = useState<TerminalTabKey>('curl')
  const [copied, setCopied] = useState(false)

  const tabs: TabConfig[] = [
    {
      key: 'curl',
      label: 'cURL',
      snippet: `export OPENAI_BASE_URL="${openAiBaseUrl}"
export OPENAI_API_KEY="sk-anyflows-..."

curl $OPENAI_BASE_URL/chat/completions \\
  -H "Authorization: Bearer $OPENAI_API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": ${JSON.stringify(sampleModel)},
    "stream": true,
    "messages": [{"role": "user", "content": "Hello AnyFlows"}]
  }'`,
      rendered: (
        <>
          <div className="text-muted-foreground/75"># {t('landing.terminalUi.configure')}</div>
          <div>
            <span className="text-primary font-medium">export </span>
            <span className="text-foreground">OPENAI_BASE_URL</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"{openAiBaseUrl}"</span>
          </div>
          <div>
            <span className="text-primary font-medium">export </span>
            <span className="text-foreground">OPENAI_API_KEY</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"sk-anyflows-..."</span>
          </div>
          <div className="mt-3 text-muted-foreground/75"># {t('landing.terminalUi.unifiedCall')}</div>
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
            <span className="text-success">{JSON.stringify(sampleModel)}</span>
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
      snippet: `export ANTHROPIC_BASE_URL="${baseUrl}"
export ANTHROPIC_API_KEY="sk-anyflows-..."

# ${t('landing.terminalUi.claudeStart')}
claude`,
      rendered: (
        <>
          <div className="text-muted-foreground/75"># {t('landing.terminalUi.claudeEnv')}</div>
          <div>
            <span className="text-primary font-medium">export </span>
            <span className="text-foreground">ANTHROPIC_BASE_URL</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"{baseUrl}"</span>
          </div>
          <div>
            <span className="text-primary font-medium">export </span>
            <span className="text-foreground">ANTHROPIC_API_KEY</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"sk-anyflows-..."</span>
          </div>
          <div className="mt-4 text-muted-foreground/75"># {t('landing.terminalUi.claudeStart')}</div>
          <div>
            <span className="text-primary font-semibold">$ claude</span>
          </div>
          <div className="mt-2 text-xs text-muted-foreground/75">
            &gt; {t('landing.terminalUi.claudeReady')}
          </div>
        </>
      ),
    },
    {
      key: 'cursor',
      label: 'Cursor',
      badge: 'IDE',
      snippet: `# ${t('landing.terminalUi.cursorSettings')}
# ${t('landing.terminalUi.cursorEnable')} [Override OpenAI Base URL]
# ${t('landing.terminalUi.cursorBaseUrl')} ${openAiBaseUrl}
# ${t('landing.terminalUi.cursorApiKey')} sk-anyflows-...
# ${t('landing.terminalUi.cursorModel', { model: sampleModel })}`,
      rendered: (
        <>
          <div className="text-muted-foreground/75"># {t('landing.terminalUi.cursorSettings')}</div>
          <div className="mt-1">
            <span className="text-info font-medium">{t('landing.terminalUi.cursorEnable')} </span>
            <span className="text-foreground">Override OpenAI Base URL</span>
          </div>
          <div>
            <span className="text-info font-medium">{t('landing.terminalUi.cursorBaseUrl')} </span>
            <span className="text-success font-semibold">{openAiBaseUrl}</span>
          </div>
          <div>
            <span className="text-info font-medium">{t('landing.terminalUi.cursorApiKey')} </span>
            <span className="text-success">sk-anyflows-****************</span>
          </div>
          <div className="mt-3 text-muted-foreground/75"># {t('landing.terminalUi.cursorModelHint')}</div>
          <div className="flex flex-wrap gap-1.5 pt-1">
            <span className="rounded bg-surface-2 px-2 py-0.5 text-xs text-primary font-mono">{sampleModel}</span>
          </div>
        </>
      ),
    },
    {
      key: 'python',
      label: 'Python',
      snippet: `from openai import OpenAI

client = OpenAI(
    base_url="${openAiBaseUrl}",
    api_key="sk-anyflows-...",
)

response = client.chat.completions.create(
    model=${JSON.stringify(sampleModel)},
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
            <span className="text-success">"{openAiBaseUrl}"</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">api_key</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">"sk-anyflows-..."</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div><span className="text-foreground">)</span></div>
          <div className="mt-2 text-muted-foreground/75"># {t('landing.terminalUi.officialSdk')}</div>
          <div>
            <span className="text-foreground">response </span>
            <span className="text-muted-foreground">= </span>
            <span className="text-foreground">client.chat.completions.create(</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">model</span>
            <span className="text-muted-foreground">=</span>
            <span className="text-success">{JSON.stringify(sampleModel)}</span>
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
  baseURL: '${openAiBaseUrl}',
  apiKey: process.env.ANYFLOWS_API_KEY,
})

const { textStream } = streamText({
  model: anyflows.chat(${JSON.stringify(sampleModel)}),
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
            <span className="text-success">'{openAiBaseUrl}'</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div className="pl-4">
            <span className="text-foreground">apiKey</span>
            <span className="text-muted-foreground">: </span>
            <span className="text-foreground">process.env.ANYFLOWS_API_KEY</span>
            <span className="text-muted-foreground">,</span>
          </div>
          <div><span className="text-foreground">{`})`}</span></div>
          <div className="mt-2 text-muted-foreground/75"># {t('landing.terminalUi.vercelSdk')}</div>
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
            <span className="text-foreground">anyflows.chat(</span>
            <span className="text-success">{JSON.stringify(sampleModel)}</span>
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
              <span>{t('landing.catalog.example')}</span>
            </span>
          </div>
        </div>

        {/* Subbar with Language/Client Tabs */}
        <div className="flex items-center justify-between border-b border-[var(--hairline)] bg-surface-1/60 px-3 py-1.5 overflow-x-auto scrollbar-none">
          <div className="flex items-center gap-1">
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
              <span className="text-primary font-bold">✓</span> {t('landing.terminalUi.protocols')}
            </span>
            <span className="inline-flex items-center gap-1 text-foreground/80">
              <span className="text-success font-bold">✓</span> {t('landing.terminalUi.retry')}
            </span>
            <span className="inline-flex items-center gap-1 text-foreground/80">
              <span className="text-info font-bold">✓</span> {t('landing.terminalUi.streaming')}
            </span>
          </div>
          <span className="hidden sm:inline font-mono text-[0.625rem] text-muted-foreground/60">
            {t('landing.terminalUi.example')}
          </span>
        </div>
      </div>
    </div>
  )
}
