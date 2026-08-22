import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'

test('退款审批入口使用管理员路由', () => {
  assert.deepEqual(routeFromHash('#/console/refunds'), { view: 'refunds' })
  assert.deepEqual(routeFromHash('#/console/refunds?ignored=true'), { view: 'refunds' })
})

test('退款审批 API 与 OpenAPI 保持四个操作', async () => {
  const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
  assert.equal(openapi.paths['/api/admin/refunds'].get.operationId, 'listAdminRefunds')
  assert.equal(openapi.paths['/api/admin/refunds/{request_id}/approve'].post.operationId, 'approveAdminRefund')
  assert.equal(openapi.paths['/api/admin/refunds/{request_id}/reject'].post.operationId, 'rejectAdminRefund')
  assert.equal(openapi.paths['/api/admin/refunds/{request_id}/submit'].post.operationId, 'submitAdminRefund')
  assert.equal(openapi.components.schemas.AdminRefundRequest.properties.payment_reference, undefined)
})

test('退款审批页保留结果未知和人工提交状态提示', async () => {
  const source = await readFile(new URL('../src/features/refunds/refund-page.tsx', import.meta.url), 'utf8')
  assert.match(source, /refund_outcome_unknown/)
  assert.match(source, /useSubmitAdminRefund/)
  assert.match(source, /approval_status === 'approved'/)
})

test('退款审批中英文文案均完整', async () => {
  const locales = await Promise.all(['zh', 'en'].map(async (locale) => JSON.parse(
    await readFile(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'),
  )))
  for (const locale of locales) {
    assert.equal(typeof locale.nav.refunds, 'string')
    assert.equal(typeof locale.refunds.title, 'string')
    assert.equal(typeof locale.refunds.errors.refund_outcome_unknown, 'string')
  }
})
