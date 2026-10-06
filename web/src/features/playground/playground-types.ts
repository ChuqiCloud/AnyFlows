import type { PlaygroundProtocol } from './playground-protocol'

export type PlaygroundMessageStatus = 'complete' | 'streaming' | 'cancelled' | 'interrupted' | 'error'

export type PlaygroundMessage = {
  id: string
  role: 'user' | 'assistant'
  content: string
  status: PlaygroundMessageStatus
}

export type PlaygroundUsage = {
  inputTokens?: number
  outputTokens?: number
  totalTokens?: number
}

export type PlaygroundErrorKind =
  | 'invalid_key'
  | 'insufficient_quota'
  | 'rate_limited'
  | 'model_unavailable'
  | 'invalid_request'
  | 'upstream_unavailable'
  | 'unknown'

export type PlaygroundRequestState = 'idle' | 'streaming' | 'complete' | 'cancelled' | 'interrupted' | 'error'

export type PlaygroundOverallState = PlaygroundRequestState | 'partial'

export type PlaygroundGenerationSettings = {
  model: string
  protocol: PlaygroundProtocol
  systemPrompt: string
  temperature?: number
  maxOutputTokens?: number
}

export type PlaygroundSharedSettings = Omit<PlaygroundGenerationSettings, 'model' | 'protocol'>

export type PlaygroundSession = {
  model: string
  messages: PlaygroundMessage[]
  requestState: PlaygroundRequestState
  errorKind?: PlaygroundErrorKind
  usage?: PlaygroundUsage
}

export const MAX_PLAYGROUND_MODELS = 4
