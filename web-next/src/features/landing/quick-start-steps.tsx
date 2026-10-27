import {
  ArrowRight,
  Boxes,
  Check,
  Code2,
  Copy,
  KeyRound,
  Sparkles,
  Terminal,
} from 'lucide-react'
import { useState } from 'react'
import { Link as RouterLink } from 'react-router-dom'

import { copyText } from '@/lib/clipboard'

import { useLandingBaseUrl } from './use-landing-models'

export function QuickStartSteps() {
  const [copiedKey, setCopiedKey] = useState(false)
  const baseUrl = useLandingBaseUrl()

  const handleCopySampleKey = async () => {
    const success = await copyText(`export OPENAI_BASE_URL="${baseUrl}/v1"\nexport OPENAI_API_KEY="sk-anyflows-..."`)
    if (success) {
      setCopiedKey(true)
      setTimeout(() => setCopiedKey(false), 2000)
    }
  }

  return (
    <section className="border-t border-[var(--hairline)] bg-surface-sunken/40 py-20 sm:py-24">
      <div className="mx-auto w-full max-w-[1240px] px-4 sm:px-6 lg:px-8">
        {/* Header */}
        <div className="flex flex-col sm:flex-row sm:items-end justify-between gap-6">
          <div>
            <div className="inline-flex items-center gap-1.5 font-mono text-xs font-semibold uppercase tracking-[0.2em] text-primary">
              <Sparkles className="size-3.5" />
              <span>QUICK START · 快速开始</span>
            </div>
            <h2 className="mt-3 text-3xl font-bold tracking-tight text-foreground sm:text-4xl">
              配置并完成首次调用
            </h2>
            <p className="mt-2.5 max-w-2xl text-sm leading-6 text-muted-foreground sm:text-base">
              无需繁琐审批，从创建密钥到在开发工具中发出第一条请求，只需 4 步，5 分钟跑通。
            </p>
          </div>

          <RouterLink
            to="/api"
            className="inline-flex items-center gap-2 text-sm font-semibold text-primary hover:text-primary/80 transition-colors shrink-0 group"
          >
            <span>查看完整 API 规范</span>
            <ArrowRight className="size-4 transition-transform group-hover:translate-x-1" />
          </RouterLink>
        </div>

        {/* 4 Steps Grid */}
        <div className="mt-12 grid gap-6 md:grid-cols-2 lg:grid-cols-4">
          {/* STEP 01 */}
          <div className="relative flex flex-col justify-between rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs transition-all hover:border-primary/40 hover:-translate-y-1">
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-primary tracking-wider">
                  STEP 01
                </span>
                <Boxes className="size-4 text-muted-foreground" />
              </div>
              <h3 className="mt-4 text-base font-semibold text-foreground">
                选择模型与线路策略
              </h3>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                在模型广场根据任务需求选择模型（Claude 3.7、GPT-4o、DeepSeek），并挑选适用的线路（极速线路或经济线路）。
              </p>
            </div>

            <div className="mt-6 rounded-xl border border-[var(--hairline)] bg-surface-sunken p-3 text-xs">
              <div className="flex items-center justify-between text-muted-foreground text-[0.6875rem]">
                <span>Claude 3.7 Sonnet</span>
                <span className="text-success font-mono font-medium">99.98%</span>
              </div>
              <div className="mt-2 flex items-center justify-between font-mono text-[0.6875rem]">
                <span className="text-foreground">主力极速线路</span>
                <span className="text-primary font-medium">82ms TTFT</span>
              </div>
            </div>
          </div>

          {/* STEP 02 */}
          <div className="relative flex flex-col justify-between rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs transition-all hover:border-primary/40 hover:-translate-y-1">
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-primary tracking-wider">
                  STEP 02
                </span>
                <KeyRound className="size-4 text-muted-foreground" />
              </div>
              <h3 className="mt-4 text-base font-semibold text-foreground">
                创建访问密钥 (API Key)
              </h3>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                在控制台一键生成专属 API Key，支持绑定指定模型分组、配额预算上限与安全白名单。
              </p>
            </div>

            <div className="mt-6 rounded-xl border border-[var(--hairline)] bg-surface-sunken p-3 font-mono text-[0.6875rem]">
              <div className="text-muted-foreground"># API Key 已就绪</div>
              <div className="mt-1 text-success truncate font-semibold">
                sk-anyflows-9x7c••••••••
              </div>
              <div className="mt-1 text-muted-foreground text-[0.625rem]">
                全渠道授权 · 自动余额计费
              </div>
            </div>
          </div>

          {/* STEP 03 */}
          <div className="relative flex flex-col justify-between rounded-2xl border border-[var(--hairline)] bg-surface-1 p-6 shadow-xs transition-all hover:border-primary/40 hover:-translate-y-1">
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-primary tracking-wider">
                  STEP 03
                </span>
                <Code2 className="size-4 text-muted-foreground" />
              </div>
              <h3 className="mt-4 text-base font-semibold text-foreground">
                选择使用场景与客户端
              </h3>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                无论是在 IDE 辅助编码、CLI 自动化 Agent 还是生产后端业务，全面原生兼容主流开发生态。
              </p>
            </div>

            <div className="mt-6 flex flex-wrap gap-1.5 pt-1">
              <span className="rounded-md border border-[var(--hairline)] bg-surface-sunken px-2 py-1 text-[0.6875rem] font-medium text-foreground">
                Claude Code
              </span>
              <span className="rounded-md border border-[var(--hairline)] bg-surface-sunken px-2 py-1 text-[0.6875rem] font-medium text-foreground">
                Cursor
              </span>
              <span className="rounded-md border border-[var(--hairline)] bg-surface-sunken px-2 py-1 text-[0.6875rem] font-medium text-foreground">
                Cline
              </span>
              <span className="rounded-md border border-[var(--hairline)] bg-surface-sunken px-2 py-1 text-[0.6875rem] font-medium text-foreground">
                OpenAI SDK
              </span>
            </div>
          </div>

          {/* STEP 04 */}
          <div className="relative flex flex-col justify-between rounded-2xl border border-primary/30 bg-gradient-to-b from-primary/8 via-surface-1 to-surface-1 p-6 shadow-xs transition-all hover:border-primary/60 hover:-translate-y-1">
            <div>
              <div className="flex items-center justify-between">
                <span className="font-mono text-xs font-semibold text-primary tracking-wider">
                  STEP 04
                </span>
                <Terminal className="size-4 text-primary" />
              </div>
              <h3 className="mt-4 text-base font-semibold text-foreground">
                配置端点，即刻发起调用
              </h3>
              <p className="mt-2 text-xs leading-5 text-muted-foreground">
                修改 Base URL 和 Key 即可，无需改动任何既有业务逻辑，流式长连接秒级返回。
              </p>
            </div>

            <div className="mt-6">
              <button
                type="button"
                onClick={handleCopySampleKey}
                className="w-full flex items-center justify-between rounded-xl border border-[var(--hairline)] bg-surface-sunken p-2.5 font-mono text-[0.6875rem] text-muted-foreground hover:text-foreground hover:border-primary/40 transition-colors cursor-pointer"
              >
                <span className="truncate">export OPENAI_BASE_URL=...</span>
                {copiedKey ? (
                  <Check className="size-3.5 text-success shrink-0" />
                ) : (
                  <Copy className="size-3.5 shrink-0" />
                )}
              </button>
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
