import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const component = await readFile(new URL('../src/features/organization-console/organization-plan-catalog.tsx', import.meta.url), 'utf8')
const purchase = await readFile(new URL('../src/features/organization-console/organization-plan-purchase.tsx', import.meta.url), 'utf8')
const api = await readFile(new URL('../src/features/organization-console/organization-plan-api.ts', import.meta.url), 'utf8')
const en = JSON.parse(await readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8'))
const zh = JSON.parse(await readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))

test('企业套餐目录 OpenAPI 与生成入口保持完整', () => {
  assert.equal(openapi.paths['/api/account/organization-plan-catalog'].get.operationId, 'getOrganizationPlanCatalog')
  assert.equal(openapi.paths['/api/admin/organization-plans'].post.operationId, 'createAdminOrganizationPlan')
  assert.equal(openapi.paths['/api/organizations/{organization_id}/plan-orders'].post.operationId, 'createOrganizationPlanOrder')
  assert.equal(openapi.paths['/api/organizations/{organization_id}/plan-orders/{order_id}'].get.operationId, 'getOrganizationPlanOrder')
  assert.equal(openapi.paths['/api/organizations/{organization_id}/plan-orders/{order_id}/payment'].post.operationId, 'submitOrganizationPlanOrderPayment')
  for (const schema of [
    'OrganizationPlan',
    'OrganizationPlanCatalogResponse',
    'OrganizationPlanCreateRequest',
    'OrganizationPlanOrder',
    'OrganizationPlanOrderCreateRequest',
    'OrganizationPlanOrderPaymentRequest',
    'OrganizationPlanOrderPaymentResponse',
  ]) {
    assert.ok(openapi.components.schemas[schema], `缺少 ${schema}`)
  }
  for (const operation of [
    'getOrganizationPlanCatalog',
    'createOrganizationPlanOrder',
    'getOrganizationPlanOrder',
    'submitOrganizationPlanOrderPayment',
  ]) assert.match(api, new RegExp(operation))
})

test('企业套餐卡片展示容量、能力、有效期和价格边界', () => {
  for (const text of ['seat_limit', 'team_limit', 'organization_key_limit', 'audit_retention_days', 'validity_days', 'price_amount_minor', 'purchase_notice']) {
    assert.match(component, new RegExp(text))
  }
  assert.match(component, /不把目录误报为已购买授权/)
})

test('企业订单支付按企业与套餐隔离恢复且不持久化敏感材料', () => {
  assert.match(purchase, /payment-attempt:' \+ organizationId \+ ':' \+ planId/)
  assert.match(purchase, /organizationId !== organizationId/)
  assert.match(purchase, /sessionStorage\.setItem/)
  assert.doesNotMatch(purchase, /sessionStorage\.setItem\([^\n]+client_secret/)
  assert.match(purchase, /safePaymentRedirectUrl/)
  assert.match(purchase, /paymentPaid/)
  assert.doesNotMatch(purchase, /授权已生效/)
  assert.match(component, /useUserTopupConfiguration\(canPurchase\)/)
})

test('企业套餐目录双语文案同步且不含替换字符', () => {
  const required = [
    'eyebrow',
    'title',
    'subtitle',
    'empty',
    'minorUnit',
    'validity',
    'paymentMethod',
    'paymentPaid',
    'paymentOutcomeUnknown',
  ]
  for (const key of required) {
    const enValue = key === 'paymentOutcomeUnknown'
      ? en.organizationConsole.overview.planCatalog.errors[key]
      : en.organizationConsole.overview.planCatalog[key]
    const zhValue = key === 'paymentOutcomeUnknown'
      ? zh.organizationConsole.overview.planCatalog.errors[key]
      : zh.organizationConsole.overview.planCatalog[key]
    assert.equal(typeof enValue, 'string')
    assert.equal(typeof zhValue, 'string')
  }
  for (const value of [en.organizationConsole.overview.planCatalog, zh.organizationConsole.overview.planCatalog]) {
    assert.ok(!JSON.stringify(value).includes('\uFFFD'))
  }
})
