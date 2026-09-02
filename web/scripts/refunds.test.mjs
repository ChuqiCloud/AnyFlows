import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'

test('退款审批入口使用管理员路由', () => {
  assert.deepEqual(routeFromHash('#/console/refunds'), { view: 'refunds' })
  assert.deepEqual(routeFromHash('#/console/refunds?ignored=true'), { view: 'refunds' })
})

test('退款审批与对账 API 与 OpenAPI 保持八个操作', async () => {
  const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
  assert.equal(openapi.paths['/api/admin/refunds'].get.operationId, 'listAdminRefunds')
  assert.equal(openapi.paths['/api/admin/refunds/{request_id}/approve'].post.operationId, 'approveAdminRefund')
  assert.equal(openapi.paths['/api/admin/refunds/{request_id}/reject'].post.operationId, 'rejectAdminRefund')
  assert.equal(openapi.paths['/api/admin/refunds/{request_id}/submit'].post.operationId, 'submitAdminRefund')
  assert.equal(openapi.paths['/api/admin/refunds/{request_id}/manual-complete'].post.operationId, 'manualCompleteAdminRefund')
  assert.equal(openapi.paths['/api/account/refund-reconciliations'].get.operationId, 'listAccountRefundReconciliations')
  assert.equal(openapi.paths['/api/organizations/{organization_id}/refund-reconciliations'].get.operationId, 'listOrganizationRefundReconciliations')
  assert.equal(openapi.paths['/api/admin/refund-reconciliations'].get.operationId, 'listAdminRefundReconciliations')
  assert.equal(openapi.components.schemas.AdminRefundRequest.properties.payment_reference, undefined)
  const reconciliation = openapi.components.schemas.RefundReconciliationEntry
  assert.equal(reconciliation.properties.amount_delta_minor.maximum, -1)
  assert.equal(reconciliation.properties.provider_refund_id, undefined)
  assert.equal(reconciliation.properties.provider_event_id, undefined)
  assert.equal(reconciliation.properties.signature, undefined)
  assert.equal(reconciliation.properties.payload, undefined)
})

test('退款审批页保留结果未知和人工提交状态提示', async () => {
  const source = await readFile(new URL('../src/features/refunds/refund-page.tsx', import.meta.url), 'utf8')
  assert.match(source, /refund_outcome_unknown/)
  assert.match(source, /useSubmitAdminRefund/)
  assert.match(source, /useManualCompleteAdminRefund/)
  assert.match(source, /newCompletionKey/)
  assert.match(source, /manually_succeeded/)
  assert.match(source, /manualComplete\.mutate/)
  assert.match(source, /expected_version/)
  assert.match(source, /entry.provider === 'epay'/)
  assert.doesNotMatch(source, /provider_refund_id\}\}<\/div>/)
  assert.match(source, /approval_status === 'approved'/)
})

test('人工退款文案保持双语且覆盖失败结果', async () => {
  const zh = JSON.parse(await readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))
  const en = JSON.parse(await readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8'))
  for (const locale of [zh, en]) {
    assert.equal(typeof locale.refunds.manual.title, 'string')
    assert.equal(typeof locale.refunds.manual.referencePlaceholder, 'string')
    assert.equal(typeof locale.refunds.manual.failed, 'string')
    assert.equal(typeof locale.refunds.statuses.manually_failed, 'string')
  }
})

test('易支付不注册自动退款 Provider 且人工开关由运行时快照控制', async () => {
  const runtime = await readFile(new URL('../../crates/af-server/src/payment_runtime.rs', import.meta.url), 'utf8')
  assert.doesNotMatch(runtime, /EasyPayRefundProvider/)
  assert.match(runtime, /manual_refund_enabled/)
  assert.match(runtime, /epay_manual_refund_enabled/)
})

test('退款对账列表复用稳定游标并展示脱敏资金主体', async () => {
  const source = await readFile(new URL('../src/features/refunds/refund-reconciliation-list.tsx', import.meta.url), 'utf8')
  const api = await readFile(new URL('../src/features/refunds/refund-reconciliation-api.ts', import.meta.url), 'utf8')
  assert.match(source, /scope === 'admin'/)
  assert.match(source, /ownerOrganization/)
  assert.match(source, /amount_delta_minor/)
  assert.doesNotMatch(source, /provider_refund_id|provider_event_id|signature|payload/)
  assert.match(api, /useInfiniteQuery/)
  assert.match(api, /before: pageParam/)
})

test('退款审批中英文文案均完整', async () => {
  const locales = await Promise.all(['zh', 'en'].map(async (locale) => JSON.parse(
    await readFile(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'),
  )))
  for (const locale of locales) {
    assert.equal(typeof locale.nav.refunds, 'string')
    assert.equal(typeof locale.refunds.title, 'string')
    assert.equal(typeof locale.refunds.views.reconciliation, 'string')
    assert.equal(typeof locale.refunds.reconciliation.columns.owner, 'string')
    assert.equal(typeof locale.wallet.reconciliation.title, 'string')
    assert.equal(typeof locale.organizationConsole.wallet.reconciliation.title, 'string')
    assert.equal(typeof locale.refunds.errors.refund_outcome_unknown, 'string')
  }
})
