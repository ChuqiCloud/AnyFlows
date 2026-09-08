import assert from 'node:assert/strict'
import { test } from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  classifyPublicShareError,
  classifyShareMutationError,
} from '../src/features/playground/playground-share-errors.ts'
import { buildPlaygroundShareSnapshot } from '../src/features/playground/playground-share-snapshot.ts'

function message(id, role, content, status = 'complete') {
  return { id, role, content, status }
}

function session(messages, overrides = {}) {
  return {
    model: 'gpt-5',
    messages,
    requestState: 'complete',
    usage: { inputTokens: 12, outputTokens: 34 },
    errorKind: 'upstream_unavailable',
    ...overrides,
  }
}

test('公开分享路由只接受完整的固定格式令牌', () => {
  const token = `sh-af-${'A'.repeat(43)}`
  assert.deepEqual(routeFromHash(`#/share/${token}`), { view: 'share', shareToken: token })
  assert.deepEqual(routeFromHash('#/share/short'), { view: 'share', shareToken: undefined })
  assert.deepEqual(routeFromHash(`#/share/${token}/extra`), { view: 'share', shareToken: undefined })
  assert.deepEqual(routeFromHash('#/console/playground'), { view: 'playground' })
  assert.deepEqual(routeFromHash('#/console/api-keys'), { view: 'api-keys' })
  assert.deepEqual(routeFromHash('#/console/users'), { view: 'users' })
})

test('分享快照只保留完成往返且不携带隐藏会话字段', () => {
  const result = buildPlaygroundShareSnapshot([
    session([
      message('u1', 'user', 'first question'),
      message('a1', 'assistant', 'first answer'),
      message('u2', 'user', 'failed question'),
      message('a2', 'assistant', 'partial answer', 'error'),
    ]),
  ])

  assert.equal(result.ok, true)
  assert.deepEqual(result.sessions, [{
    model: 'gpt-5',
    messages: [
      { role: 'user', content: 'first question' },
      { role: 'assistant', content: 'first answer' },
    ],
  }])
  const serialized = JSON.stringify(result)
  assert.doesNotMatch(serialized, /usage|errorKind|inputTokens|partial answer/)
})

test('分享快照拒绝没有完成往返和超出消息容量的结果', () => {
  const incomplete = buildPlaygroundShareSnapshot([
    session([
      message('u1', 'user', 'question'),
      message('a1', 'assistant', '', 'error'),
    ], { requestState: 'error' }),
  ])
  assert.deepEqual(incomplete, { ok: false, reason: 'no_complete_rounds' })

  const messages = Array.from({ length: 258 }, (_, index) => (
    message(String(index), index % 2 === 0 ? 'user' : 'assistant', `message-${index}`)
  ))
  assert.deepEqual(
    buildPlaygroundShareSnapshot([session(messages)]),
    { ok: false, reason: 'too_large' },
  )
})

test('分享快照按 UTF-8 字节检查单条正文', () => {
  const result = buildPlaygroundShareSnapshot([
    session([
      message('u1', 'user', '问'.repeat(22_000)),
      message('a1', 'assistant', 'answer'),
    ]),
  ])

  assert.deepEqual(result, { ok: false, reason: 'invalid_content' })
})

test('分享错误只把明确的 404 归类为链接失效', () => {
  assert.equal(classifyPublicShareError({ status: 404, secret: 'hidden' }), 'not_found')
  assert.equal(classifyPublicShareError({ status: 500 }), 'unavailable')
  assert.equal(classifyPublicShareError(new TypeError('network failed')), 'unavailable')
  assert.equal(classifyShareMutationError({ status: 400 }), 'invalid')
  assert.equal(classifyShareMutationError({ status: 409 }), 'limit')
  assert.equal(classifyShareMutationError({ status: 503 }), 'unavailable')
})
