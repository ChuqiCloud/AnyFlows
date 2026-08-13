import assert from 'node:assert/strict'
import test from 'node:test'

import { resolvePlaygroundStreamResult } from '../src/features/playground/playground-stream-result.ts'

test('没有正文时保留原始流错误', async () => {
  const streamError = new Error('upstream failed')
  await assert.rejects(resolvePlaygroundStreamResult({
    finishReason: Promise.resolve('unknown'),
    streamError,
    textReceived: false,
    usage: Promise.resolve({}),
  }), streamError)
})

test('已有正文时把尾帧错误降级为部分响应', async () => {
  const streamError = new Error('upstream failed')
  assert.deepEqual(await resolvePlaygroundStreamResult({
    finishReason: Promise.reject(streamError),
    streamError,
    textReceived: true,
    usage: Promise.reject(streamError),
  }), {
    finishReason: 'unknown',
    interrupted: true,
    usage: {},
  })
})

test('正文迭代器抛错时仍按部分响应处理', async () => {
  const streamError = new Error('stream iterator failed')
  assert.deepEqual(await resolvePlaygroundStreamResult({
    finishReason: Promise.resolve('unknown'),
    streamError,
    textReceived: true,
    usage: Promise.resolve({}),
  }), {
    finishReason: 'unknown',
    interrupted: true,
    usage: {},
  })
})

test('完整终态保留停止原因与用量', async () => {
  assert.deepEqual(await resolvePlaygroundStreamResult({
    finishReason: Promise.resolve('stop'),
    streamError: undefined,
    textReceived: true,
    usage: Promise.resolve({ inputTokens: 4, outputTokens: 8, totalTokens: 12 }),
  }), {
    finishReason: 'stop',
    interrupted: false,
    usage: { inputTokens: 4, outputTokens: 8, totalTokens: 12 },
  })
})
