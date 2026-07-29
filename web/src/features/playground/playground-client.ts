import { createOpenAICompatible } from '@ai-sdk/openai-compatible'
import { streamText, type ModelMessage } from 'ai'

import type {
  PlaygroundGenerationSettings,
  PlaygroundMessage,
  PlaygroundUsage,
} from './playground-types'
import { playgroundRequestContext } from './playground-comparison-state'

type PlaygroundStreamRequest = PlaygroundGenerationSettings & {
  messages: PlaygroundMessage[]
  signal: AbortSignal
  onText: (delta: string) => void
}

export type PlaygroundStreamResult = {
  finishReason: string
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
  const provider = createOpenAICompatible({
    name: 'anyflows',
    apiKey: request.apiKey,
    baseURL: `${getApiRoot()}/v1`,
    includeUsage: true,
    fetch: (input, init) => fetch(input, { ...init, credentials: 'same-origin' }),
  })
  const messages: ModelMessage[] = playgroundRequestContext(request.messages)
    .map((message) => ({ role: message.role, content: message.content }))
  let streamError: unknown

  const result = streamText({
    model: provider.chatModel(request.model),
    instructions: request.systemPrompt.trim() || undefined,
    messages,
    temperature: request.temperature,
    maxOutputTokens: request.maxOutputTokens,
    maxRetries: 0,
    abortSignal: request.signal,
    onError: ({ error }) => {
      streamError ??= error
    },
  })

  for await (const delta of result.textStream) {
    request.onText(delta)
  }
  if (streamError) throw streamError

  const [usage, finishReason] = await Promise.all([result.usage, result.finishReason])
  return {
    finishReason,
    usage: {
      inputTokens: usage.inputTokens,
      outputTokens: usage.outputTokens,
      totalTokens: usage.totalTokens,
    },
  }
}
