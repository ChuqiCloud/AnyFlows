import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const component = await readFile(new URL('../src/features/organization-console/organization-plan-catalog.tsx', import.meta.url), 'utf8')
const api = await readFile(new URL('../src/features/organization-console/organization-plan-api.ts', import.meta.url), 'utf8')
const en = JSON.parse(await readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8'))
const zh = JSON.parse(await readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))

test('企业套餐目录 OpenAPI 与生成入口保持完整', () => {
  assert.equal(openapi.paths['/api/account/organization-plan-catalog'].get.operationId, 'getOrganizationPlanCatalog')
  assert.equal(openapi.paths['/api/admin/organization-plans'].post.operationId, 'createAdminOrganizationPlan')
  for (const schema of ['OrganizationPlan', 'OrganizationPlanCatalogResponse', 'OrganizationPlanCreateRequest']) {
    assert.ok(openapi.components.schemas[schema], `缺少 ${schema}`)
  }
  assert.match(api, /getOrganizationPlanCatalog/)
})

test('企业套餐卡片展示容量、能力、有效期和价格边界', () => {
  for (const text of ['seat_limit', 'team_limit', 'organization_key_limit', 'audit_retention_days', 'validity_days', 'price_amount_minor', 'purchase_notice']) {
    assert.match(component, new RegExp(text))
  }
  assert.match(component, /不把目录误报为已购买授权/)
})

test('企业套餐目录双语文案同步且不含替换字符', () => {
  const required = ['eyebrow', 'title', 'subtitle', 'empty', 'minorUnit', 'validity']
  for (const key of required) {
    assert.equal(typeof en.organizationConsole.overview.planCatalog[key], 'string')
    assert.equal(typeof zh.organizationConsole.overview.planCatalog[key], 'string')
  }
  for (const value of [en.organizationConsole.overview.planCatalog, zh.organizationConsole.overview.planCatalog]) {
    assert.ok(!JSON.stringify(value).includes('\uFFFD'))
  }
})
