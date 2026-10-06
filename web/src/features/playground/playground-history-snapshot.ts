import type { PlaygroundShareSession } from '@/lib/api/generated/types.gen'
import type { PlaygroundSession } from './playground-types'

/** 为新会话生成非零 128 位小写十六进制标识。 */
export function createPlaygroundConversationId() {
  const bytes = new Uint8Array(16)
  do {
    globalThis.crypto.getRandomValues(bytes)
  } while (bytes.every((byte) => byte === 0))
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

/** 生成仅驻留内存的稳定指纹，正文不得进入 Query key 或日志。 */
export function playgroundHistoryFingerprint(sessions: PlaygroundShareSession[]) {
  return JSON.stringify({ version: 1, sessions })
}

/** 把服务端已验证的完整往返恢复为当前页面会话状态。 */
export function restorePlaygroundSessions(
  sessions: PlaygroundShareSession[],
): PlaygroundSession[] {
  return sessions.map((session) => ({
    model: session.model,
    messages: session.messages.map((message) => ({
      id: globalThis.crypto.randomUUID(),
      role: message.role,
      content: message.content,
      status: 'complete',
    })),
    requestState: 'complete',
  }))
}
