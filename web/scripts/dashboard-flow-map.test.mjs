import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const flowMap = await readFile(new URL('../src/features/dashboard/dashboard-flow-map.tsx', import.meta.url), 'utf8')
const outcomes = await readFile(new URL('../../crates/af-db/src/admin_dashboard_outcomes.rs', import.meta.url), 'utf8')
const zh = JSON.parse(await readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))
const en = JSON.parse(await readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8'))

function schema(name) {
  return openapi.components?.schemas?.[name]
}

function readPath(object, path) {
  return path.split('.').reduce((value, key) => value?.[key], object)
}

test('四层流向契约包含低敏路径字段和额度统计', () => {
  const response = schema('AdminDashboardResponse')
  const path = schema('AdminDashboardFlowPath')
  assert.ok(path)
  assert.deepEqual(path.required, [
    'user_id',
    'group_id',
    'group_name',
    'channel_id',
    'channel_name',
    'model',
    'request_count',
    'quota_consumed',
  ])
  assert.deepEqual(Object.keys(path.properties).sort(), [
    'channel_id',
    'channel_name',
    'group_id',
    'group_name',
    'model',
    'quota_consumed',
    'request_count',
    'user_id',
  ])
  assert.equal(response.properties.flow_paths.items.$ref, '#/components/schemas/AdminDashboardFlowPath')
  assert.equal(response.properties.flow_request_count.type, 'integer')
  assert.equal(response.properties.flow_quota_consumed.type, 'integer')
})

test('流向图同时支持请求量和额度模式，并限制为二十四条路径', () => {
  assert.match(flowMap, /type FlowMode = 'requests' \| 'quota'/)
  assert.match(flowMap, /mode === 'requests' \? path\.request_count : path\.quota_consumed/)
  assert.match(flowMap, /\bflow_paths\b/)
  assert.match(outcomes, /MAX_DASHBOARD_FLOW_PATHS: usize = 24/)
  assert.match(flowMap, /overflow-x-auto/)
})

test('用户标签使用低敏编号且中英文文案完整', () => {
  assert.match(flowMap, /dashboard\.outcomes\.flowMap\.user/)
  assert.match(flowMap, /userLabel\(path\.user_id\)/)
  for (const locale of [zh, en]) {
    assert.equal(typeof readPath(locale, 'dashboard.outcomes.flowMap.user'), 'string')
    assert.equal(typeof readPath(locale, 'dashboard.outcomes.flowMap.modes.requests'), 'string')
    assert.equal(typeof readPath(locale, 'dashboard.outcomes.flowMap.modes.quota'), 'string')
    assert.equal(typeof readPath(locale, 'dashboard.outcomes.flowMap.empty'), 'string')
  }
})

test('流向前端不包含正文、请求头、Token 或凭据字段', () => {
  for (const forbidden of ['request_body', 'requestBody', 'headers', 'token', 'secret', 'credential', 'payload']) {
    assert.equal(flowMap.toLowerCase().includes(forbidden.toLowerCase()), false, `发现敏感字段: ${forbidden}`)
  }
})
