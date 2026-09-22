import type { PlaygroundShareSession } from '@/lib/api/generated/types.gen'
import type { PlaygroundMessage, PlaygroundSession } from './playground-types.ts'

const MAX_MODEL_BYTES = 256
const MAX_MESSAGE_BYTES = 64 * 1024
const MAX_MESSAGES = 256
const MAX_SNAPSHOT_BYTES = 512 * 1024

export type PlaygroundShareSnapshotResult =
  | { ok: true; sessions: PlaygroundShareSession[] }
  | { ok: false; reason: 'no_complete_rounds' | 'invalid_content' | 'too_large' }

function completedRoundTrips(messages: PlaygroundMessage[]) {
  const visible: PlaygroundShareSession['messages'] = []

  for (let index = 0; index < messages.length; index += 2) {
    const user = messages[index]
    const assistant = messages[index + 1]
    if (user?.role !== 'user' || assistant?.role !== 'assistant') {
      return undefined
    }
    if (user.status === 'complete' && assistant.status === 'complete') {
      visible.push(
        { role: 'user', content: user.content },
        { role: 'assistant', content: assistant.content },
      )
    }
  }

  return visible
}

function utf8Bytes(value: string) {
  return new TextEncoder().encode(value).byteLength
}

/** 构造公开快照，只复制完成的可见往返，不接触凭据、参数、用量或错误信息。 */
export function buildPlaygroundShareSnapshot(
  sessions: PlaygroundSession[],
): PlaygroundShareSnapshotResult {
  const snapshot: PlaygroundShareSession[] = []
  let messageCount = 0

  for (const session of sessions) {
    const messages = completedRoundTrips(session.messages)
    if (!messages) return { ok: false, reason: 'invalid_content' }
    if (messages.length === 0) continue
    if (utf8Bytes(session.model) > MAX_MODEL_BYTES || session.model.trim() !== session.model) {
      return { ok: false, reason: 'invalid_content' }
    }
    if (messages.some((message) => (
      message.content.length === 0
      || message.content.includes('\0')
      || utf8Bytes(message.content) > MAX_MESSAGE_BYTES
    ))) {
      return { ok: false, reason: 'invalid_content' }
    }
    messageCount += messages.length
    snapshot.push({ model: session.model, messages })
  }

  if (snapshot.length === 0) return { ok: false, reason: 'no_complete_rounds' }
  if (snapshot.length > 4 || messageCount > MAX_MESSAGES) {
    return { ok: false, reason: 'too_large' }
  }
  const storedSnapshot = JSON.stringify({ version: 1, sessions: snapshot })
  if (utf8Bytes(storedSnapshot) > MAX_SNAPSHOT_BYTES) {
    return { ok: false, reason: 'too_large' }
  }

  return { ok: true, sessions: snapshot }
}
