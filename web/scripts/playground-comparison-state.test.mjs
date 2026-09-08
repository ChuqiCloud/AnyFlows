import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  createPlaygroundSession,
  playgroundOverallState,
  playgroundRequestContext,
  playgroundRetryTurn,
  playgroundSessionReducer,
} from '../src/features/playground/playground-comparison-state.ts'
import { isPlaygroundTranscriptNearBottom } from '../src/features/playground/playground-scroll.ts'
import { normalizePlaygroundInstructions } from '../src/features/playground/playground-instructions.ts'
import {
  DEFAULT_PLAYGROUND_COMPARISON_ENABLED,
  modelsForPlaygroundSelectionMode,
  normalizePlaygroundModels,
  selectPlaygroundModel,
  togglePlaygroundModel,
} from '../src/features/playground/playground-model-selection.ts'
import {
  filterPlaygroundModels,
  playgroundModelProviderCategories,
} from '../src/features/playground/playground-model-filter.ts'
import {
  preferredPlaygroundProtocol,
  resolvePlaygroundTargets,
} from '../src/features/playground/playground-protocol.ts'

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

test('试炼场默认使用单选且新模型直接替换当前模型', () => {
  assert.equal(DEFAULT_PLAYGROUND_COMPARISON_ENABLED, false)
  assert.deepEqual(selectPlaygroundModel(['model-a'], 'model-b', false), ['model-b'])
  assert.deepEqual(
    modelsForPlaygroundSelectionMode(['model-a', 'model-b'], false),
    ['model-a'],
  )
})

test('显式开启对比后才允许多选，关闭时收敛到主模型', () => {
  assert.deepEqual(selectPlaygroundModel(['model-a'], 'model-b', true), ['model-a', 'model-b'])
  assert.deepEqual(selectPlaygroundModel(['model-a', 'model-b'], 'model-a', true), ['model-b'])
  assert.deepEqual(
    modelsForPlaygroundSelectionMode(['model-b', 'model-c'], false),
    ['model-b'],
  )
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

test('替换模型时保留已完成上下文并继续使用当前会话', () => {
  const sessions = [{
    model: 'model-a',
    requestState: 'complete',
    usage: { inputTokens: 10, outputTokens: 20, totalTokens: 30 },
    messages: [
      message('user-a', 'user', 'hello'),
      message('assistant-a', 'assistant', 'world'),
    ],
  }]
  const switched = playgroundSessionReducer(sessions, {
    type: 'sync',
    models: ['model-b'],
  })

  assert.equal(switched[0].model, 'model-b')
  assert.deepEqual(switched[0].messages.map((item) => item.content), ['hello', 'world'])
  assert.equal(switched[0].requestState, 'complete')
  assert.equal(switched[0].usage, undefined)
})

test('增加比较模型时保留原列并让新增列从当前轮开始', () => {
  const original = {
    model: 'model-a',
    requestState: 'complete',
    messages: [
      message('user-a', 'user', 'hello'),
      message('assistant-a', 'assistant', 'world'),
    ],
  }
  const expanded = playgroundSessionReducer([original], {
    type: 'sync',
    models: ['model-a', 'model-b'],
  })

  assert.equal(expanded[0], original)
  assert.equal(expanded[1].model, 'model-b')
  assert.equal(expanded[1].messages.length, 0)
  assert.equal(expanded[1].requestState, 'idle')
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

test('失败轮次重试复用原用户消息并清空助手残留正文', () => {
  const failedSession = {
    model: 'model-a',
    requestState: 'error',
    errorKind: 'upstream_unavailable',
    messages: [
      message('user-a', 'user', 'first'),
      message('assistant-a', 'assistant', 'answer'),
      message('user-b', 'user', 'retry me'),
      message('assistant-b', 'assistant', 'partial', 'error'),
    ],
  }
  const turn = playgroundRetryTurn(failedSession)
  assert.equal(turn.assistantId, 'assistant-b')
  assert.deepEqual(turn.requestMessages.map((item) => item.id), ['user-a', 'assistant-a', 'user-b'])

  const retried = playgroundSessionReducer([failedSession], {
    type: 'retry',
    model: 'model-a',
    assistantId: 'assistant-b',
  })[0]
  assert.equal(retried.requestState, 'streaming')
  assert.equal(retried.errorKind, undefined)
  assert.equal(retried.messages[3].status, 'streaming')
  assert.equal(retried.messages[3].content, '')
})

test('仅在对话视口贴近底部时保持流式跟随', () => {
  assert.equal(isPlaygroundTranscriptNearBottom({ scrollHeight: 1000, scrollTop: 428, clientHeight: 500 }), true)
  assert.equal(isPlaygroundTranscriptNearBottom({ scrollHeight: 1000, scrollTop: 300, clientHeight: 500 }), false)
  assert.equal(isPlaygroundTranscriptNearBottom({ scrollHeight: 400, scrollTop: 0, clientHeight: 500 }), true)
})

test('空白系统提示词不会注入请求', () => {
  assert.equal(normalizePlaygroundInstructions(''), undefined)
  assert.equal(normalizePlaygroundInstructions('  \n\t  '), undefined)
  assert.equal(normalizePlaygroundInstructions('  请简洁回答  '), '请简洁回答')
})

test('Anthropic-only 模型使用 Messages 协议进入试炼场', () => {
  assert.equal(preferredPlaygroundProtocol(['anthropic']), 'anthropic')
  assert.deepEqual(resolvePlaygroundTargets(
    ['gpt-5.5', 'deepseek-v4-flash'],
    { 'gpt-5.5': 'openai_responses', 'deepseek-v4-flash': 'anthropic' },
  ), [
    { model: 'gpt-5.5', protocol: 'openai_responses' },
    { model: 'deepseek-v4-flash', protocol: 'anthropic' },
  ])
})

test('模型选择分类组合供应商与协议过滤', () => {
  const models = [
    { model: 'gpt-5.5', provider: 'openai', available_protocols: ['openai_responses'] },
    { model: 'deepseek-v4-flash', provider: 'deepseek', available_protocols: ['anthropic'] },
    { model: 'deepseek-v4-pro', provider: 'deepseek', available_protocols: ['anthropic'] },
  ]
  assert.deepEqual(playgroundModelProviderCategories(models), [
    { value: 'all', count: 3 },
    { value: 'deepseek', count: 2 },
    { value: 'openai', count: 1 },
  ])
  assert.deepEqual(
    filterPlaygroundModels(models, { provider: 'deepseek', protocol: 'anthropic' })
      .map((item) => item.model),
    ['deepseek-v4-flash', 'deepseek-v4-pro'],
  )
  assert.deepEqual(
    filterPlaygroundModels(models, { provider: 'openai', protocol: 'anthropic' }),
    [],
  )
})
