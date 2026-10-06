import assert from 'node:assert/strict'
import test from 'node:test'

import {
  buildModelSyncDraftSchema,
  defaultModelSyncDraft,
  modelSyncDraftIsComplete,
  toModelSyncApplyItem,
} from '../src/features/model-management/model-sync-form-model.ts'

const messages = {
  model: 'model',
  displayName: 'displayName',
  provider: 'provider',
  description: 'description',
  iconUrl: 'iconUrl',
  tags: 'tags',
  contextWindow: 'contextWindow',
  modalities: 'modalities',
}

test('同步草稿默认不猜测厂商或模型能力', () => {
  const draft = defaultModelSyncDraft()

  assert.equal(draft.provider, '')
  assert.deepEqual(draft.inputModalities, [])
  assert.deepEqual(draft.outputModalities, [])
  assert.equal(draft.supportsReasoning, false)
  assert.equal(draft.supportsToolCalls, false)
  assert.equal(modelSyncDraftIsComplete(draft), false)
})

test('同步草稿只有补齐权威身份和双向模态后才能应用', () => {
  const draft = {
    ...defaultModelSyncDraft(),
    displayName: 'Verified model',
    provider: 'verified-provider',
    inputModalities: ['text'],
    outputModalities: ['text'],
  }

  assert.equal(buildModelSyncDraftSchema(messages).safeParse(draft).success, true)
  assert.equal(modelSyncDraftIsComplete(draft), true)
  assert.equal(modelSyncDraftIsComplete({ ...draft, inputModalities: [] }), false)
  assert.equal(modelSyncDraftIsComplete({ ...draft, outputModalities: [] }), false)
})

test('应用请求只提交管理员确认值并保留固定预览条目标识', () => {
  const draft = {
    ...defaultModelSyncDraft(),
    displayName: '  Verified model  ',
    provider: '  verified-provider  ',
    contextWindow: '200000',
    inputModalities: ['text', 'image'],
    outputModalities: ['text'],
    supportsReasoning: true,
  }
  const item = {
    item_id: 17,
    canonical_model: 'canonical-model',
    upstream_model: 'upstream-model',
    relation: 'missing_metadata',
    display_name_hint: 'Unverified upstream name',
    description_hint: null,
    context_window_hint: 999999,
    input_token_limit_hint: null,
    output_token_limit_hint: null,
    supported_methods: ['generateContent'],
    applied_model_id: null,
  }

  const request = toModelSyncApplyItem(item, draft)

  assert.equal(request.item_id, 17)
  assert.equal(request.display_name, 'Verified model')
  assert.equal(request.provider, 'verified-provider')
  assert.equal(request.context_window, 200000)
  assert.deepEqual(request.input_modalities, ['text', 'image'])
  assert.equal(request.supports_reasoning, true)
})
