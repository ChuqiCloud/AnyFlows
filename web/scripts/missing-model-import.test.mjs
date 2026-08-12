import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import {
  defaultMissingModelImportDraft,
  missingDisplayNamesAreValid,
  toMissingModelImportRequest,
} from '../src/features/model-management/missing-model-form-model.ts'

const model = {
  model: 'gpt-5.5',
  channel_count: 1,
  channels: [{ channel_id: 7, channel_name: 'OpenAI upstream' }],
}

test('缺失模型批次默认不猜测厂商或能力', () => {
  const draft = defaultMissingModelImportDraft()

  assert.equal(draft.provider, '')
  assert.deepEqual(draft.inputModalities, [])
  assert.deepEqual(draft.outputModalities, [])
  assert.equal(draft.supportsReasoning, false)
  assert.equal(draft.supportsToolCalls, false)
})

test('快速导入请求只包含明确元数据并固定不提交运营状态', () => {
  const request = toMissingModelImportRequest(
    [model],
    { 'gpt-5.5': ' GPT 5.5 ' },
    {
      provider: ' openai ',
      inputModalities: ['text'],
      outputModalities: ['text'],
      supportsReasoning: true,
      supportsToolCalls: true,
    },
  )

  assert.equal(missingDisplayNamesAreValid([model], { 'gpt-5.5': 'GPT 5.5' }), true)
  assert.deepEqual(request.items[0], {
    model: 'gpt-5.5',
    display_name: 'GPT 5.5',
    provider: 'openai',
    description: null,
    icon_url: null,
    tags: [],
    context_window: null,
    input_modalities: ['text'],
    output_modalities: ['text'],
    supports_reasoning: true,
    supports_tool_calls: true,
  })
  assert.equal('visibility' in request.items[0], false)
  assert.equal('lifecycle' in request.items[0], false)
})

test('OpenAPI 只在缺失模型资源暴露批量导入操作', () => {
  const openapi = JSON.parse(readFileSync(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))

  assert.equal(openapi.paths['/api/admin/models/missing'].post.operationId, 'importMissingAdminModels')
  assert.equal(openapi.paths['/api/auth/session'].post, undefined)
  assert.equal(
    openapi.paths['/api/admin/models/missing'].post.requestBody.content['application/json'].schema.$ref,
    '#/components/schemas/AdminMissingModelImportRequest',
  )
})
