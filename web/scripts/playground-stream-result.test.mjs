import assert from 'node:assert/strict'
import test from 'node:test'
import { setImmediate as nextEventLoop } from 'node:timers/promises'

import { resolvePlaygroundStreamResult } from '../src/features/playground/playground-stream-result.ts'
import { resolvePlaygroundStreamResult as resolveNextPlaygroundStreamResult } from '../../web-next/src/features/playground/playground-stream-result.ts'

test('没有正文时保留原始流错误', async () => {
  const streamError = new Error('upstream failed')
  await assert.rejects(resolvePlaygroundStreamResult({
    finishReason: Promise.resolve('unknown'),
    streamError,
    textReceived: false,
    usage: Promise.resolve({}),
  }), streamError)
})

for (const [frontend, resolveStreamResult] of [
  ['Classic', resolvePlaygroundStreamResult],
  ['Next', resolveNextPlaygroundStreamResult],
]) {
  test(`${frontend} 首段正文前失败时消费元数据拒绝并保留原始错误`, async () => {
    const streamError = new Error('original stream failure')
    await assert.rejects(resolveStreamResult({
      finishReason: Promise.reject(new Error('finish reason failed')),
      streamError,
      textReceived: false,
      usage: Promise.reject(new Error('usage failed')),
    }), error => error === streamError)
    // 让未处理的拒绝进入 node:test 的检测阶段，不安装全局错误监听器。
    await nextEventLoop()
  })

  test(`${frontend} 首段正文前的原始错误不等待元数据结算`, async () => {
    const streamError = new Error('original stream failure')
    let resolveUsage
    let resolveFinishReason
    const result = resolveStreamResult({
      finishReason: new Promise(resolve => { resolveFinishReason = resolve }),
      streamError,
      textReceived: false,
      usage: new Promise(resolve => { resolveUsage = resolve }),
    })
    try {
      const outcome = await Promise.race([
        result.then(
          () => ({ status: 'fulfilled' }),
          error => ({ status: 'rejected', error }),
        ),
        nextEventLoop({ status: 'pending' }),
      ])
      assert.equal(outcome.status, 'rejected', '原始流错误必须在元数据仍 pending 时返回')
      assert.equal(outcome.error, streamError)
    } finally {
      resolveUsage({})
      resolveFinishReason('unknown')
      await Promise.allSettled([result])
    }
  })
}

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
