import Anthropic from '@lobehub/icons/es/Anthropic/components/Mono'
import DeepSeek from '@lobehub/icons/es/DeepSeek/components/Mono'
import Gemini from '@lobehub/icons/es/Gemini/components/Mono'
import Grok from '@lobehub/icons/es/Grok/components/Mono'
import OpenAI from '@lobehub/icons/es/OpenAI/components/Mono'
import { ArrowRight, Check, Copy, ExternalLink, Sparkles } from 'lucide-react'
import type { ComponentType } from 'react'
import { useState } from 'react'
import { Link as RouterLink } from 'react-router-dom'

import { copyText } from '@/lib/clipboard'
import { cn } from '@/lib/utils'

interface FeaturedModel {
  id: string
  name: string
  provider: string
  context: string
  group: string
  groupType: 'primary' | 'ultra' | 'reasoning' | 'eco'
  inputPrice: string
  outputPrice: string
  latency: string
  Icon: ComponentType<{ className?: string }>
}

const featuredModels: FeaturedModel[] = [
  {
    id: 'claude-opus-5-5',
    name: 'Claude Opus 5.5',
    provider: 'Anthropic',
    context: '200K',
    group: '旗舰推理',
    groupType: 'ultra',
    inputPrice: '¥18.00',
    outputPrice: '¥90.00',
    latency: '86ms',
    Icon: Anthropic,
  },
  {
    id: 'gpt-6.1-sol',
    name: 'GPT-6.1 Sol',
    provider: 'OpenAI',
    context: '400K',
    group: '旗舰通用',
    groupType: 'ultra',
    inputPrice: '¥12.00',
    outputPrice: '¥60.00',
    latency: '71ms',
    Icon: OpenAI,
  },
  {
    id: 'claude-sonnet-5-5',
    name: 'Claude Sonnet 5.5',
    provider: 'Anthropic',
    context: '200K',
    group: '主力极速',
    groupType: 'primary',
    inputPrice: '¥3.60',
    outputPrice: '¥18.00',
    latency: '64ms',
    Icon: Anthropic,
  },
  {
    id: 'deepseek-flash',
    name: 'DeepSeek Flash',
    provider: 'DeepSeek',
    context: '128K',
    group: '深度推理',
    groupType: 'reasoning',
    inputPrice: '¥0.38',
    outputPrice: '¥1.52',
    latency: '92ms',
    Icon: DeepSeek,
  },
  {
    id: 'gemini-3.8-flash',
    name: 'Gemini 3.8 Flash',
    provider: 'Google',
    context: '1M',
    group: '极速低延时',
    groupType: 'primary',
    inputPrice: '¥0.07',
    outputPrice: '¥0.28',
    latency: '58ms',
    Icon: Gemini,
  },
  {
    id: 'grok-4.7',
    name: 'Grok 4.7',
    provider: 'xAI',
    context: '256K',
    group: '成本优先',
    groupType: 'eco',
    inputPrice: '¥0.25',
    outputPrice: '¥0.75',
    latency: '68ms',
    Icon: Grok,
  },
]

export function FeaturedModelsTable() {
  const [copiedId, setCopiedId] = useState<string | null>(null)

  const handleCopy = async (id: string) => {
    const success = await copyText(id)
    if (success) {
      setCopiedId(id)
      setTimeout(() => setCopiedId(null), 2000)
    }
  }

  return (
    <section className="mx-auto w-full max-w-[1240px] px-4 py-20 sm:px-6 sm:py-24 lg:px-8">
      {/* Section Header */}
      <div className="flex flex-col sm:flex-row sm:items-end justify-between gap-6 border-b border-[var(--hairline)] pb-8">
        <div>
          <div className="inline-flex items-center gap-1.5 font-mono text-xs font-semibold uppercase tracking-[0.2em] text-primary">
            <Sparkles className="size-3.5" />
            <span>MODELS & PRICING · 模型与价格</span>
          </div>
          <h2 className="mt-3 text-3xl font-bold tracking-tight text-foreground sm:text-4xl">
            常用模型与透明价格矩阵
          </h2>
          <p className="mt-2.5 max-w-2xl text-sm leading-6 text-muted-foreground sm:text-base">
            价格单位为人民币 / 百万 Token，为示例折算价。最新模型与实时价格请以模型广场为准。
          </p>
        </div>

        <RouterLink
          to="/models"
          className="inline-flex items-center gap-2 text-sm font-semibold text-primary hover:text-primary/80 transition-colors shrink-0 group"
        >
          <span>查看全部模型广场</span>
          <ArrowRight className="size-4 transition-transform group-hover:translate-x-1" />
        </RouterLink>
      </div>

      {/* Pricing Matrix Table */}
      <div className="mt-8 overflow-x-auto rounded-2xl border border-[var(--hairline)] bg-surface-1/70 shadow-xs backdrop-blur-sm">
        <table className="w-full min-w-[760px] text-left border-collapse">
          <thead>
            <tr className="border-b border-[var(--hairline)] bg-surface-sunken/60 text-[0.6875rem] font-mono uppercase tracking-[0.12em] text-muted-foreground">
              <th className="py-3.5 px-6 font-semibold">模型与架构</th>
              <th className="py-3.5 px-5 font-semibold">推荐线路分组</th>
              <th className="py-3.5 px-5 font-semibold">输入价格 (1M)</th>
              <th className="py-3.5 px-5 font-semibold">输出价格 (1M)</th>
              <th className="py-3.5 px-5 font-semibold">首字延迟</th>
              <th className="py-3.5 px-6 font-semibold text-right">模型标识</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-[var(--hairline)] text-sm">
            {featuredModels.map((item) => {
              const Icon = item.Icon
              const isCopied = copiedId === item.id

              return (
                <tr
                  key={item.id}
                  className="transition-colors hover:bg-surface-2/60 group"
                >
                  {/* Model Name & Provider */}
                  <td className="py-4 px-6">
                    <div className="flex items-center gap-3">
                      <div className="size-8 rounded-lg bg-surface-2 border border-[var(--hairline)] flex items-center justify-center shrink-0">
                        <Icon className="size-4.5 text-foreground" />
                      </div>
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="font-semibold text-foreground group-hover:text-primary transition-colors">
                            {item.name}
                          </span>
                          <span className="rounded bg-surface-2 px-1.5 py-0.5 text-[0.625rem] font-mono text-muted-foreground border border-[var(--hairline)]">
                            {item.context}
                          </span>
                        </div>
                        <span className="text-xs text-muted-foreground">
                          {item.provider}
                        </span>
                      </div>
                    </div>
                  </td>

                  {/* Group Tag */}
                  <td className="py-4 px-5">
                    <span
                      className={cn(
                        'inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs font-medium border',
                        item.groupType === 'primary' && 'bg-primary/10 border-primary/25 text-primary',
                        item.groupType === 'ultra' && 'bg-success/10 border-success/25 text-success',
                        item.groupType === 'reasoning' && 'bg-amber-500/10 border-amber-500/25 text-amber-500 dark:text-amber-400',
                        item.groupType === 'eco' && 'bg-info/10 border-info/25 text-info',
                      )}
                    >
                      <span className="size-1 rounded-full bg-current" />
                      {item.group}
                    </span>
                  </td>

                  {/* Input Price */}
                  <td className="py-4 px-5 font-mono text-xs font-medium text-foreground">
                    {item.inputPrice}
                  </td>

                  {/* Output Price */}
                  <td className="py-4 px-5 font-mono text-xs font-medium text-foreground">
                    {item.outputPrice}
                  </td>

                  {/* Latency */}
                  <td className="py-4 px-5 font-mono text-xs text-success">
                    <span className="inline-flex items-center gap-1.5">
                      <span className="size-1.5 rounded-full bg-success animate-pulse" />
                      {item.latency}
                    </span>
                  </td>

                  {/* Copy Action */}
                  <td className="py-4 px-6 text-right">
                    <button
                      type="button"
                      onClick={() => handleCopy(item.id)}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-[var(--hairline)] bg-surface-2/80 px-2.5 py-1 text-xs font-mono text-muted-foreground transition-all hover:border-primary/40 hover:bg-surface-2 hover:text-foreground active:scale-95 cursor-pointer"
                      title="点击复制模型 ID"
                    >
                      {isCopied ? (
                        <>
                          <Check className="size-3 text-success" />
                          <span className="text-success font-medium">已复制</span>
                        </>
                      ) : (
                        <>
                          <Copy className="size-3" />
                          <span>{item.id}</span>
                        </>
                      )}
                    </button>
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>

      {/* Micro-notes */}
      <div className="mt-4 flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground px-1">
        <span>* 线路均提供心跳探活与 99.98% SLA 保障，单请求异常时自动秒级降级。</span>
        <RouterLink to="/models" className="hover:text-foreground inline-flex items-center gap-1 font-medium">
          查看倍率与完整 80+ 款模型 <ExternalLink className="size-3" />
        </RouterLink>
      </div>
    </section>
  )
}
