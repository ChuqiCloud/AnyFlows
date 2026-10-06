import { createAnthropic } from '@ai-sdk/anthropic'
import { createOpenAI } from '@ai-sdk/openai'
import { createOpenAICompatible } from '@ai-sdk/openai-compatible'
import { streamText, type ModelMessage } from 'ai'

import { apiBaseUrl } from '@/lib/api/endpoint'
import { getManagementSessionToken, invalidateManagementSession } from '@/lib/api/session-token'

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
  return apiBaseUrl || window.location.origin
}

/** 使用登录会话进入试炼场鉴权、计费与调度链，并逐段回传文本。 */
export async function streamPlaygroundChat(
  request: PlaygroundStreamRequest,
): Promise<PlaygroundStreamResult> {
  const fetchWithCredentials = (input: RequestInfo | URL, init?: RequestInit) => {
    const headers = new Headers(init?.headers)
    const sessionToken = getManagementSessionToken()
    headers.delete('x-api-key')
    headers.delete('x-goog-api-key')
    if (sessionToken) headers.set('Authorization', `Bearer ${sessionToken}`)
    return fetch(input, { ...init, headers, credentials: 'same-origin' }).then((response) => {
      if (response.status === 401) invalidateManagementSession()
      return response
    })
  }
  const baseURL = `${getApiRoot()}/api/playground/v1`
  const provider = createOpenAICompatible({
    name: 'anyflows',
    apiKey: 'session-auth',
    baseURL,
    includeUsage: true,
    fetch: fetchWithCredentials,
  })
  const responsesProvider = createOpenAI({
    name: 'anyflows',
    apiKey: 'session-auth',
    baseURL,
    fetch: fetchWithCredentials,
  })
  const anthropicProvider = createAnthropic({
    apiKey: 'session-auth',
    baseURL,
    fetch: fetchWithCredentials,
  })
  const messages: ModelMessage[] = playgroundRequestContext(request.messages)
    .map((message) => ({ role: message.role, content: message.content }))
  let streamError: unknown
  let textReceived = false

  const model = request.protocol === 'openai_responses'
    ? responsesProvider.responses(request.model)
    : request.protocol === 'anthropic'
      ? anthropicProvider.messages(request.model)
      : provider.chatModel(request.model)
  const result = streamText({
    model,
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
