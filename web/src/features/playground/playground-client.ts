import { createOpenAI } from '@ai-sdk/openai'
import { createOpenAICompatible } from '@ai-sdk/openai-compatible'
import { streamText, type ModelMessage } from 'ai'

import type {
  PlaygroundGenerationSettings,
  PlaygroundMessage,
  PlaygroundUsage,
} from './playground-types'
import type { PlaygroundProtocol } from './playground-protocol'
import { playgroundRequestContext } from './playground-comparison-state'
import { normalizePlaygroundInstructions } from './playground-instructions'
import { resolvePlaygroundStreamResult } from './playground-stream-result'

type PlaygroundStreamRequest = Omit<PlaygroundGenerationSettings, 'protocol'> & {
  messages: PlaygroundMessage[]
  signal: AbortSignal
  onText: (delta: string) => void
  protocol: PlaygroundProtocol
}

export type PlaygroundStreamResult = {
  finishReason: string
  interrupted: boolean
  usage: PlaygroundUsage
}

function getApiRoot() {
  const configured = import.meta.env.VITE_API_BASE_URL?.trim().replace(/\/+$/, '')
  return configured || window.location.origin
}

/** 使用普通 API Key 进入现有鉴权、计费与调度链，并逐段回传文本。 */
export async function streamPlaygroundChat(
  request: PlaygroundStreamRequest,
): Promise<PlaygroundStreamResult> {
  const fetchWithCredentials = (input: RequestInfo | URL, init?: RequestInit) => (
    fetch(input, { ...init, credentials: 'same-origin' })
  )
  const provider = createOpenAICompatible({
    name: 'anyflows',
    apiKey: request.apiKey,
    baseURL: `${getApiRoot()}/v1`,
    includeUsage: true,
    fetch: fetchWithCredentials,
  })
  const responsesProvider = createOpenAI({
    name: 'anyflows',
    apiKey: request.apiKey,
    baseURL: `${getApiRoot()}/v1`,
    fetch: fetchWithCredentials,
  })
  const messages: ModelMessage[] = playgroundRequestContext(request.messages)
    .map((message) => ({ role: message.role, content: message.content }))
  let streamError: unknown
  let textReceived = false

  const result = streamText({
    model: request.protocol === 'openai_responses'
      ? responsesProvider.responses(request.model)
      : provider.chatModel(request.model),
    instructions: normalizePlaygroundInstructions(request.systemPrompt),
    messages,
    temperature: request.temperature,
    maxOutputTokens: request.maxOutputTokens,
    maxRetries: 0,
    abortSignal: request.signal,
    // AnyFlows 当前只允许无状态 Responses；显式关闭 SDK 默认的 store=true。
    providerOptions: request.protocol === 'openai_responses'
      ? { openai: { store: false } }
      : undefined,
    onError: ({ error }) => {
      streamError ??= error
    },
  })

  try {
    for await (const delta of result.textStream) {
      textReceived ||= delta.length > 0
      request.onText(delta)
    }
  } catch (error) {
    // AI SDK 可能在正文已到达后才从迭代器抛出协议尾帧错误，交给统一降级判定。
    streamError ??= error
  }
  return resolvePlaygroundStreamResult({
    finishReason: result.finishReason,
    streamError,
    textReceived,
    usage: Promise.resolve(result.usage).then((usage) => ({
      inputTokens: usage.inputTokens,
      outputTokens: usage.outputTokens,
      totalTokens: usage.totalTokens,
    })),
  })
}
