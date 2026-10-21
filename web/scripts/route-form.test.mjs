import assert from 'node:assert/strict'
import test from 'node:test'

import {
  defaultRouteFormValues,
  toRouteWriteRequest,
  validateRouteForm,
} from '../src/features/routes/route-form-model.ts'

function validValues() {
  return {
    ...defaultRouteFormValues(),
    name: '试炼场智能路由',
    modelPattern: 'smart-chat',
    mode: 'explicit_group',
    modelMapping: [{ id: 'mapping-1', source: 'smart-chat', target: 'gpt-5.5' }],
    candidates: [
      { id: 'candidate-1', channelId: 11, credentialId: 21, weight: '5', enabled: true },
      { id: 'candidate-2', channelId: 12, credentialId: 22, weight: '3', enabled: true },
    ],
  }
}

test('智能路由表单拒绝显式分组正则和损坏的模型映射', () => {
  const values = validValues()
  assert.deepEqual(validateRouteForm(values), {})
  assert.equal(
    validateRouteForm({ ...values, modelPattern: 're:^smart-' }).modelPattern,
    'invalidPattern',
  )
  assert.equal(
    validateRouteForm({
      ...values,
      modelMapping: [{ id: 'mapping-1', source: 're:(', target: 'gpt-5.5' }],
    }).mapping,
    'invalidMapping',
  )
  assert.equal(
    validateRouteForm({
      ...values,
      modelMapping: [{ id: 'mapping-1', source: 'smart-chat', target: '' }],
    }).mapping,
    'invalidMapping',
  )
  assert.equal(validateRouteForm({ ...values, modelPattern: ' smart-chat' }).modelPattern, 'invalidPattern')
  assert.equal(
    validateRouteForm({
      ...values,
      modelMapping: [{ id: 'mapping-1', source: 'smart-chat', target: 'gpt-5.5 ' }],
    }).mapping,
    'invalidMapping',
  )
})

test('候选可视顺序稳定转换为由高到低的规则内优先级', () => {
  const request = toRouteWriteRequest({ ...validValues(), strategy: 'stable_first' })
  assert.deepEqual(
    request.channels.map((candidate) => [candidate.channel_id, candidate.priority, candidate.weight]),
    [[11, 2, 5], [12, 1, 3]],
  )
  assert.deepEqual(request.model_mapping, { 'smart-chat': 'gpt-5.5' })
})

test('加权候选保持同级且渠道绑定不需要指定 Key', () => {
  const values = validValues()
  values.candidates[0].credentialId = null
  const request = toRouteWriteRequest(values)
  assert.deepEqual(request.channels.map((candidate) => [candidate.credential_id, candidate.priority]), [[null, 0], [22, 0]])
  assert.equal(validateRouteForm({ ...values, candidates: [...values.candidates, { ...values.candidates[0], id: 'duplicate', credentialId: 23 }] }).candidates, 'invalidCandidates')
})
