import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  createPlaygroundSession,
  playgroundOverallState,
  playgroundRequestContext,
  playgroundSessionReducer,
} from '../src/features/playground/playground-comparison-state.ts'
import {
  normalizePlaygroundModels,
  togglePlaygroundModel,
} from '../src/features/playground/playground-model-selection.ts'

function message(id, role, content, status = 'complete') {
  return { id, role, content, status }
}

test('比较模型集合去重并限制为四个', () => {
  assert.deepEqual(
    normalizePlaygroundModels(['a', 'b', 'a', 'c', 'd', 'e']),
    ['a', 'b', 'c', 'd'],
  )
  assert.deepEqual(togglePlaygroundModel(['a'], 'a'), ['a'])
  assert.deepEqual(togglePlaygroundModel(['a'], 'b'), ['a', 'b'])
  assert.deepEqual(togglePlaygroundModel(['a', 'b'], 'a'), ['b'])
  assert.deepEqual(togglePlaygroundModel(['a', 'b', 'c', 'd'], 'e'), ['a', 'b', 'c', 'd'])
})

test('并行流片段只写入所属模型列', () => {
  const sessions = ['model-a', 'model-b'].map(createPlaygroundSession)
  const started = playgroundSessionReducer(sessions, {
    type: 'begin',
    turns: [
      {
        model: 'model-a',
        userMessage: message('user', 'user', 'compare'),
        assistantMessage: message('assistant-a', 'assistant', '', 'streaming'),
      },
      {
        model: 'model-b',
        userMessage: message('user', 'user', 'compare'),
        assistantMessage: message('assistant-b', 'assistant', '', 'streaming'),
      },
    ],
  })
  const streamed = playgroundSessionReducer(started, {
    type: 'append',
    model: 'model-a',
    assistantId: 'assistant-a',
    delta: 'alpha',
  })

  assert.equal(streamed[0].messages[1].content, 'alpha')
  assert.equal(streamed[1].messages[1].content, '')
  assert.equal(playgroundOverallState(streamed), 'streaming')
})

test('单列失败不会覆盖其他列的成功结果', () => {
  const sessions = playgroundSessionReducer(
    ['model-a', 'model-b'].map(createPlaygroundSession),
    {
      type: 'begin',
      turns: [
        {
          model: 'model-a',
          userMessage: message('user', 'user', 'compare'),
          assistantMessage: message('assistant-a', 'assistant', '', 'streaming'),
        },
        {
          model: 'model-b',
          userMessage: message('user', 'user', 'compare'),
          assistantMessage: message('assistant-b', 'assistant', '', 'streaming'),
        },
      ],
    },
  )
  const failed = playgroundSessionReducer(sessions, {
    type: 'fail',
    model: 'model-a',
    assistantId: 'assistant-a',
    cancelled: false,
    errorKind: 'upstream_unavailable',
  })
  const completed = playgroundSessionReducer(failed, {
    type: 'complete',
    model: 'model-b',
    assistantId: 'assistant-b',
    usage: { inputTokens: 10, outputTokens: 20, totalTokens: 30 },
  })

  assert.equal(completed[0].requestState, 'error')
  assert.equal(completed[0].errorKind, 'upstream_unavailable')
  assert.equal(completed[1].requestState, 'complete')
  assert.equal(completed[1].usage.totalTokens, 30)
  assert.equal(playgroundOverallState(completed), 'partial')
})

test('清空比较会话保留模型顺序并移除所有运行状态', () => {
  const sessions = [createPlaygroundSession('model-b'), createPlaygroundSession('model-a')]
  const cleared = playgroundSessionReducer(sessions, {
    type: 'clear',
    models: ['model-b', 'model-a'],
  })

  assert.deepEqual(cleared.map((session) => session.model), ['model-b', 'model-a'])
  assert.ok(cleared.every((session) => session.requestState === 'idle'))
  assert.ok(cleared.every((session) => session.messages.length === 0))
  assert.equal(playgroundOverallState(cleared), 'idle')
})

test('下一轮上下文成对剔除失败或取消的旧往返', () => {
  const context = playgroundRequestContext([
    message('user-a', 'user', 'failed prompt'),
    message('assistant-a', 'assistant', 'partial', 'error'),
    message('user-b', 'user', 'completed prompt'),
    message('assistant-b', 'assistant', 'completed answer'),
    message('user-c', 'user', 'new prompt'),
  ])

  assert.deepEqual(context.map((item) => item.id), ['user-b', 'assistant-b', 'user-c'])
})
