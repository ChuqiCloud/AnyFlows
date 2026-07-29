export type PlaygroundMessageStatus = 'complete' | 'streaming' | 'cancelled' | 'error'

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

export type PlaygroundRequestState = 'idle' | 'streaming' | 'cancelled' | 'error'

export type PlaygroundGenerationSettings = {
  apiKey: string
  model: string
  systemPrompt: string
  temperature?: number
  maxOutputTokens?: number
}
