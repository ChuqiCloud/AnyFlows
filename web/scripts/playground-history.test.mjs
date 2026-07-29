import assert from 'node:assert/strict'
import { test } from 'node:test'

import { playgroundSessionReducer } from '../src/features/playground/playground-comparison-state.ts'
import {
  createPlaygroundConversationId,
  playgroundHistoryFingerprint,
  restorePlaygroundSessions,
} from '../src/features/playground/playground-history-snapshot.ts'

const storedSessions = [{
  model: 'gpt-5',
  messages: [
    { role: 'user', content: 'private question' },
    { role: 'assistant', content: 'private answer' },
  ],
}]

test('私有历史标识为非零的 128 位小写十六进制文本', () => {
  const first = createPlaygroundConversationId()
  const second = createPlaygroundConversationId()
  assert.match(first, /^(?!0{32}$)[0-9a-f]{32}$/)
  assert.match(second, /^(?!0{32}$)[0-9a-f]{32}$/)
  assert.notEqual(first, second)
})

test('历史恢复只生成完整消息状态并原子替换当前比较列', () => {
  const restored = restorePlaygroundSessions(storedSessions)
  const reduced = playgroundSessionReducer([], { type: 'restore', sessions: restored })

  assert.equal(reduced.length, 1)
  assert.equal(reduced[0].requestState, 'complete')
  assert.deepEqual(reduced[0].messages.map((message) => ({
    role: message.role,
    content: message.content,
    status: message.status,
  })), [
    { role: 'user', content: 'private question', status: 'complete' },
    { role: 'assistant', content: 'private answer', status: 'complete' },
  ])
  assert.notEqual(reduced[0].messages[0].id, reduced[0].messages[1].id)
})

test('历史指纹只包含版本化模型与可见消息', () => {
  const fingerprint = playgroundHistoryFingerprint(storedSessions)
  assert.deepEqual(JSON.parse(fingerprint), { version: 1, sessions: storedSessions })
  assert.doesNotMatch(fingerprint, /apiKey|systemPrompt|temperature|usage|error/)
})
