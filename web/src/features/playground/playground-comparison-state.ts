import type {
  PlaygroundErrorKind,
  PlaygroundMessage,
  PlaygroundOverallState,
  PlaygroundSession,
  PlaygroundUsage,
} from './playground-types.ts'
import { normalizePlaygroundModels } from './playground-model-selection.ts'

export type PlaygroundTurnSeed = {
  model: string
  userMessage: PlaygroundMessage
  assistantMessage: PlaygroundMessage
}

export type PlaygroundSessionAction =
  | { type: 'sync'; models: string[] }
  | { type: 'begin'; turns: PlaygroundTurnSeed[] }
  | { type: 'append'; model: string; assistantId: string; delta: string }
  | { type: 'complete'; model: string; assistantId: string; usage: PlaygroundUsage }
  | {
    type: 'fail'
    model: string
    assistantId: string
    cancelled: boolean
    errorKind?: PlaygroundErrorKind
  }
  | { type: 'clear'; models: string[] }
  | { type: 'restore'; sessions: PlaygroundSession[] }

export function createPlaygroundSession(model: string): PlaygroundSession {
  return { model, messages: [], requestState: 'idle' }
}

function updateAssistant(
  messages: PlaygroundMessage[],
  assistantId: string,
  update: (message: PlaygroundMessage) => PlaygroundMessage,
) {
  return messages.map((message) => message.id === assistantId ? update(message) : message)
}

/** 归并并行流事件；所有动作只允许修改其所属模型列。 */
export function playgroundSessionReducer(
  sessions: PlaygroundSession[],
  action: PlaygroundSessionAction,
): PlaygroundSession[] {
  if (action.type === 'sync') {
    return normalizePlaygroundModels(action.models).map((model) => (
      sessions.find((session) => session.model === model) ?? createPlaygroundSession(model)
    ))
  }
  if (action.type === 'clear') {
    return normalizePlaygroundModels(action.models).map(createPlaygroundSession)
  }
  if (action.type === 'restore') {
    return action.sessions.map((session) => ({
      ...session,
      messages: session.messages.map((message) => ({ ...message })),
    }))
  }
  if (action.type === 'begin') {
    return action.turns.map((turn) => {
      const current = sessions.find((session) => session.model === turn.model)
        ?? createPlaygroundSession(turn.model)
      return {
        ...current,
        messages: [...current.messages, turn.userMessage, turn.assistantMessage],
        requestState: 'streaming',
        errorKind: undefined,
        usage: undefined,
      }
    })
  }

  return sessions.map((session) => {
    if (session.model !== action.model) return session
    if (action.type === 'append') {
      return {
        ...session,
        messages: updateAssistant(session.messages, action.assistantId, (message) => (
          message.status === 'streaming'
            ? { ...message, content: message.content + action.delta }
            : message
        )),
      }
    }
    if (action.type === 'complete') {
      return {
        ...session,
        messages: updateAssistant(session.messages, action.assistantId, (message) => ({
          ...message,
          status: 'complete',
        })),
        requestState: 'complete',
        errorKind: undefined,
        usage: action.usage,
      }
    }
    return {
      ...session,
      messages: updateAssistant(session.messages, action.assistantId, (message) => ({
        ...message,
        status: action.cancelled ? 'cancelled' : 'error',
      })),
      requestState: action.cancelled ? 'cancelled' : 'error',
      errorKind: action.cancelled ? undefined : action.errorKind,
      usage: undefined,
    }
  })
}

export function hasPlaygroundConversation(sessions: PlaygroundSession[]) {
  return sessions.some((session) => session.messages.length > 0)
}

/** 构造下一轮上下文时成对剔除失败或取消的旧往返，并保留末尾的新用户消息。 */
export function playgroundRequestContext(messages: PlaygroundMessage[]) {
  return messages.filter((message, index) => {
    if (message.content.length === 0 || message.status !== 'complete') return false
    if (message.role !== 'user') return true

    const response = messages[index + 1]
    return response?.role !== 'assistant' || response.status === 'complete'
  })
}

export function playgroundOverallState(sessions: PlaygroundSession[]): PlaygroundOverallState {
  if (!hasPlaygroundConversation(sessions)) return 'idle'
  if (sessions.some((session) => session.requestState === 'streaming')) return 'streaming'

  const completed = sessions.filter((session) => session.requestState === 'complete').length
  const failed = sessions.filter((session) => session.requestState === 'error').length
  const cancelled = sessions.filter((session) => session.requestState === 'cancelled').length
  if (completed === sessions.length) return 'complete'
  if (completed > 0) return 'partial'
  if (failed > 0 && cancelled > 0) return 'error'
  if (failed > 0) return 'error'
  if (cancelled > 0) return 'cancelled'
  return 'idle'
}
