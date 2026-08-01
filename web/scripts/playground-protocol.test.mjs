import assert from 'node:assert/strict'
import test from 'node:test'

import {
  preferredPlaygroundProtocol,
  resolvePlaygroundTargets,
} from '../src/features/playground/playground-protocol.ts'

test('试炼场优先使用 Chat，Responses-only 模型走 Responses', () => {
  assert.equal(preferredPlaygroundProtocol(['openai_chat', 'openai_responses']), 'openai_chat')
  assert.equal(preferredPlaygroundProtocol(['openai_responses']), 'openai_responses')
  assert.equal(preferredPlaygroundProtocol(['anthropic', 'gemini']), undefined)
})

test('试炼场拒绝缺少已解析协议的模型集合', () => {
  assert.deepEqual(
    resolvePlaygroundTargets(
      ['chat-model', 'responses-model'],
      { 'chat-model': 'openai_chat', 'responses-model': 'openai_responses' },
    ),
    [
      { model: 'chat-model', protocol: 'openai_chat' },
      { model: 'responses-model', protocol: 'openai_responses' },
    ],
  )
  assert.equal(
    resolvePlaygroundTargets(['chat-model', 'missing-model'], { 'chat-model': 'openai_chat' }),
    undefined,
  )
})
