import Anthropic from '@lobehub/icons/es/Anthropic/components/Mono'
import Baichuan from '@lobehub/icons/es/Baichuan/components/Mono'
import Cohere from '@lobehub/icons/es/Cohere/components/Mono'
import DeepSeek from '@lobehub/icons/es/DeepSeek/components/Mono'
import Doubao from '@lobehub/icons/es/Doubao/components/Mono'
import Gemini from '@lobehub/icons/es/Gemini/components/Mono'
import Groq from '@lobehub/icons/es/Groq/components/Mono'
import Hunyuan from '@lobehub/icons/es/Hunyuan/components/Mono'
import Kimi from '@lobehub/icons/es/Kimi/components/Mono'
import Meta from '@lobehub/icons/es/Meta/components/Mono'
import Minimax from '@lobehub/icons/es/Minimax/components/Mono'
import Mistral from '@lobehub/icons/es/Mistral/components/Mono'
import Moonshot from '@lobehub/icons/es/Moonshot/components/Mono'
import Ollama from '@lobehub/icons/es/Ollama/components/Mono'
import OpenAI from '@lobehub/icons/es/OpenAI/components/Mono'
import OpenRouter from '@lobehub/icons/es/OpenRouter/components/Mono'
import Perplexity from '@lobehub/icons/es/Perplexity/components/Mono'
import Qwen from '@lobehub/icons/es/Qwen/components/Mono'
import Spark from '@lobehub/icons/es/Spark/components/Mono'
import Stepfun from '@lobehub/icons/es/Stepfun/components/Mono'
import Together from '@lobehub/icons/es/Together/components/Mono'
import Wenxin from '@lobehub/icons/es/Wenxin/components/Mono'
import XAI from '@lobehub/icons/es/XAI/components/Mono'
import Zhipu from '@lobehub/icons/es/Zhipu/components/Mono'
import type { ComponentType, SVGProps } from 'react'
import Ai21 from '@lobehub/icons/es/Ai21/components/Mono'
import Alibaba from '@lobehub/icons/es/Alibaba/components/Mono'
import Aws from '@lobehub/icons/es/Aws/components/Mono'
import Azure from '@lobehub/icons/es/Azure/components/Mono'
import Baidu from '@lobehub/icons/es/Baidu/components/Mono'
import ByteDance from '@lobehub/icons/es/ByteDance/components/Mono'
import Cerebras from '@lobehub/icons/es/Cerebras/components/Mono'
import Fireworks from '@lobehub/icons/es/Fireworks/components/Mono'
import Google from '@lobehub/icons/es/Google/components/Mono'
import HuggingFace from '@lobehub/icons/es/HuggingFace/components/Mono'
import Jina from '@lobehub/icons/es/Jina/components/Mono'
import Microsoft from '@lobehub/icons/es/Microsoft/components/Mono'
import Nvidia from '@lobehub/icons/es/Nvidia/components/Mono'
import SambaNova from '@lobehub/icons/es/SambaNova/components/Mono'
import SiliconCloud from '@lobehub/icons/es/SiliconCloud/components/Mono'
import Tencent from '@lobehub/icons/es/Tencent/components/Mono'

import { findProvider, normalizeProvider } from './provider-catalog'

export type LogoComponent = ComponentType<SVGProps<SVGSVGElement> & { size?: number }>

/**
 * 网关已覆盖 / 规划覆盖的模型厂商标识。
 * 统一用 lobehub 的 Mono 变体（走 currentColor），便于按主题与透明度精确控制，
 * 彩色版在密集排布时会显得杂乱，不符合当前的克制取向。
 */
export const modelLogos: readonly { name: string; Icon: LogoComponent }[] = [
  { name: 'OpenAI', Icon: OpenAI as LogoComponent },
  { name: 'Anthropic', Icon: Anthropic as LogoComponent },
  { name: 'Gemini', Icon: Gemini as LogoComponent },
  { name: 'DeepSeek', Icon: DeepSeek as LogoComponent },
  { name: 'Qwen', Icon: Qwen as LogoComponent },
  { name: 'Moonshot', Icon: Moonshot as LogoComponent },
  { name: 'Kimi', Icon: Kimi as LogoComponent },
  { name: 'Zhipu', Icon: Zhipu as LogoComponent },
  { name: 'MiniMax', Icon: Minimax as LogoComponent },
  { name: 'Doubao', Icon: Doubao as LogoComponent },
  { name: 'Hunyuan', Icon: Hunyuan as LogoComponent },
  { name: 'Wenxin', Icon: Wenxin as LogoComponent },
  { name: 'Spark', Icon: Spark as LogoComponent },
  { name: 'Baichuan', Icon: Baichuan as LogoComponent },
  { name: 'Stepfun', Icon: Stepfun as LogoComponent },
  { name: 'xAI', Icon: XAI as LogoComponent },
  { name: 'Meta', Icon: Meta as LogoComponent },
  { name: 'Mistral', Icon: Mistral as LogoComponent },
  { name: 'Cohere', Icon: Cohere as LogoComponent },
  { name: 'Perplexity', Icon: Perplexity as LogoComponent },
  { name: 'Groq', Icon: Groq as LogoComponent },
  { name: 'Together', Icon: Together as LogoComponent },
  { name: 'OpenRouter', Icon: OpenRouter as LogoComponent },
  { name: 'Ollama', Icon: Ollama as LogoComponent },
  { name: 'AI21', Icon: Ai21 as LogoComponent },
  { name: 'Alibaba', Icon: Alibaba as LogoComponent },
  { name: 'AWS', Icon: Aws as LogoComponent },
  { name: 'Azure', Icon: Azure as LogoComponent },
  { name: 'Baidu', Icon: Baidu as LogoComponent },
  { name: 'ByteDance', Icon: ByteDance as LogoComponent },
  { name: 'Cerebras', Icon: Cerebras as LogoComponent },
  { name: 'Fireworks', Icon: Fireworks as LogoComponent },
  { name: 'Google', Icon: Google as LogoComponent },
  { name: 'HuggingFace', Icon: HuggingFace as LogoComponent },
  { name: 'Jina', Icon: Jina as LogoComponent },
  { name: 'Microsoft', Icon: Microsoft as LogoComponent },
  { name: 'NVIDIA', Icon: Nvidia as LogoComponent },
  { name: 'SambaNova', Icon: SambaNova as LogoComponent },
  { name: 'SiliconCloud', Icon: SiliconCloud as LogoComponent },
  { name: 'Tencent', Icon: Tencent as LogoComponent },
]

const logoByName = new Map(modelLogos.map((logo) => [logo.name, logo]))

export function findLogoByName(value: string) {
  const normalized = normalizeProvider(value)
  if (!normalized) return undefined
  return modelLogos.find((logo) => normalizeProvider(logo.name) === normalized)
}

/**
 * 仅根据模型名提供界面识别标识，不把结果当作后端供应商或能力声明。
 * 未命中稳定命名约定时返回 undefined，由调用方展示通用模型图标。
 */
export function findModelLogo(model: string) {
  const value = model.toLowerCase()
  const brand = value.includes('claude')
    ? 'Anthropic'
    : value.includes('gemini')
      ? 'Gemini'
      : value.includes('deepseek')
        ? 'DeepSeek'
        : value.includes('qwen')
          ? 'Qwen'
          : value.includes('kimi')
            ? 'Kimi'
            : value.includes('moonshot')
              ? 'Moonshot'
              : value.includes('grok')
                ? 'xAI'
                : value.includes('llama')
                  ? 'Meta'
                  : value.includes('mistral') || value.includes('codestral')
                    ? 'Mistral'
                    : value.includes('command-r') || value.includes('cohere')
                      ? 'Cohere'
                      : value.includes('doubao')
                        ? 'Doubao'
                        : value.includes('hunyuan')
                          ? 'Hunyuan'
                          : value.includes('minimax')
                            ? 'MiniMax'
                            : value.includes('wenxin') || value.includes('ernie')
                              ? 'Wenxin'
                              : value.includes('baichuan')
                                ? 'Baichuan'
                                : value.includes('stepfun')
                                  ? 'Stepfun'
                                  : value.startsWith('gpt-')
                                      || /^o[1-9](?:-|$)/.test(value)
                                      || value.includes('openai')
                                    ? 'OpenAI'
                                    : undefined
  return brand ? logoByName.get(brand) : undefined
}

/** 根据权威厂商字段选择界面图标，不从模型名推断后端能力或归属。 */
export function findProviderLogo(provider: string) {
  const brand = findProvider(provider)
  const logo = brand
    ? findLogoByName(brand.logo) ?? findLogoByName(brand.id) ?? findLogoByName(brand.name)
    : findLogoByName(provider)
  return logo ? { ...logo, name: brand?.name ?? logo.name } : undefined
}

/** Configured identity takes precedence, followed by the key, name and aliases. */
export function resolveProviderLogo(...values: (string | null | undefined)[]) {
  for (const value of values) {
    if (!value) continue
    const logo = findLogoByName(value) ?? findProviderLogo(value)
    if (logo) return logo
  }
  return undefined
}
