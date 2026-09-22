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
]

const logoByName = new Map(modelLogos.map((logo) => [logo.name, logo]))

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

const providerLogoNames: Readonly<Record<string, string>> = {
  openai: 'OpenAI',
  anthropic: 'Anthropic',
  google: 'Gemini',
  gemini: 'Gemini',
  deepseek: 'DeepSeek',
  qwen: 'Qwen',
  alibaba: 'Qwen',
  moonshot: 'Moonshot',
  kimi: 'Kimi',
  zhipu: 'Zhipu',
  minimax: 'MiniMax',
  doubao: 'Doubao',
  bytedance: 'Doubao',
  hunyuan: 'Hunyuan',
  tencent: 'Hunyuan',
  wenxin: 'Wenxin',
  baidu: 'Wenxin',
  xai: 'xAI',
  meta: 'Meta',
  mistral: 'Mistral',
  cohere: 'Cohere',
  perplexity: 'Perplexity',
  groq: 'Groq',
  together: 'Together',
  openrouter: 'OpenRouter',
  ollama: 'Ollama',
}

/** 根据权威厂商字段选择界面图标，不从模型名推断后端能力或归属。 */
export function findProviderLogo(provider: string) {
  const normalized = provider.toLowerCase().replace(/[^a-z0-9]/g, '')
  const name = providerLogoNames[normalized]
  return name ? logoByName.get(name) : undefined
}
