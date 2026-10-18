import assert from 'node:assert/strict'
import test from 'node:test'
import { compatibleProfile, scopedCursors } from '../src/features/client-profiles/client-profile-policy.ts'

test('rollback choices validate the frozen revision rather than the latest catalog entry', () => {
  const channel = { type: 'opencode', protocol: 'openai_chat' }
  const oldRevision = { service: 'open_code', protocols: ['anthropic'] }
  const currentRevision = { service: 'open_code', protocols: ['openai_chat'] }
  assert.equal(compatibleProfile(oldRevision, channel), false)
  assert.equal(compatibleProfile(currentRevision, channel), true)
  assert.equal(compatibleProfile(currentRevision, { type: 'openai', protocol: 'openai_chat' }), false)
  assert.equal(compatibleProfile({ service: 'standard', protocols: ['openai_chat'] }, channel), true)
  assert.equal(compatibleProfile({ service: 'codex', protocols: ['openai_responses'] }, { type: 'openai', protocol: 'openai_chat' }), false)
})

test('search and status filters cannot reuse a cursor from a different result set', () => {
  const state = { scope: 'enabled:open_code', cursors: [undefined, 'cursor-a', 'cursor-b'] }
  assert.deepEqual(scopedCursors(state, 'enabled:open_code'), [undefined, 'cursor-a', 'cursor-b'])
  assert.deepEqual(scopedCursors(state, 'disabled:open_code'), [undefined])
  assert.deepEqual(scopedCursors(state, 'enabled:standard'), [undefined])
  assert.deepEqual(state.cursors, [undefined, 'cursor-a', 'cursor-b'])
})
