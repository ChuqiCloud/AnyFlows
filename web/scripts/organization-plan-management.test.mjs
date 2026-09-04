import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const page = await readFile(new URL('../src/features/organization-console/organization-plan-management-page.tsx', import.meta.url), 'utf8')
const api = await readFile(new URL('../src/features/organization-console/organization-plan-api.ts', import.meta.url), 'utf8')
const route = await readFile(new URL('../src/app-route.ts', import.meta.url), 'utf8')
const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))

test('企业套餐管理入口接入路由和生成客户端 API', () => {
  assert.match(route, /organization-plans/)
  assert.match(api, /listAdminOrganizationPlans/)
  assert.match(api, /createAdminOrganizationPlan/)
  assert.match(api, /disableAdminOrganizationPlan/)
  assert.equal(openapi.paths['/api/admin/organization-plans'].post.operationId, 'createAdminOrganizationPlan')
  assert.equal(openapi.paths['/api/admin/organization-plans/{plan_id}/disable'].post.operationId, 'disableAdminOrganizationPlan')
})

test('管理端保留价格快照并按版本 CAS 停用', () => {
  assert.match(page, /price_amount_minor/)
  assert.match(page, /expected_version: plan\.version/)
  assert.match(page, /customRolesUnavailable/)
  assert.match(page, /organizationPlans\.disable\.body/)
})

test('管理端双语文案和稳定错误状态均存在', async () => {
  for (const locale of ['zh', 'en']) {
    const document = JSON.parse(await readFile(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'))
    assert.equal(typeof document.organizationPlans.title, 'string')
    assert.equal(typeof document.organizationPlans.errors.create, 'string')
    assert.ok(!JSON.stringify(document.organizationPlans).includes('\uFFFD'))
  }
  assert.match(page, /query\.isPending/)
  assert.match(page, /query\.isError/)
})
