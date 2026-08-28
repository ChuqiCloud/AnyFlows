import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { URL } from 'node:url'

import { routeFromHash } from '../src/app-route.ts'

test('服务账号管理页使用企业控制台路由并保持导航激活态', () => {
  assert.deepEqual(routeFromHash('#/console/organization-service-accounts'), {
    view: 'organization-service-accounts',
  })
})

test('服务账号前端覆盖创建、状态 CAS、轮换和预算绑定入口', () => {
  const page = readFileSync(new URL('../src/features/organization-service-accounts/organization-service-account-page.tsx', import.meta.url), 'utf8')
  const api = readFileSync(new URL('../src/features/organization-service-accounts/organization-service-account-api.ts', import.meta.url), 'utf8')
  assert.match(page, /organization\.service_accounts\.read/)
  assert.match(page, /organization\.service_accounts\.manage/)
  assert.match(page, /OrganizationServiceAccountBudgetSheet/)
  assert.match(page, /expected_version/)
  assert.match(api, /status === 404/)
})

test('一次性密钥只存在内存并在对话框关闭后由页面释放', () => {
  const page = readFileSync(new URL('../src/features/organization-service-accounts/organization-service-account-page.tsx', import.meta.url), 'utf8')
  const dialog = readFileSync(new URL('../src/features/organization-service-accounts/organization-service-account-key-dialog.tsx', import.meta.url), 'utf8')
  assert.match(page, /useState<OrganizationServiceAccountIssuedResponse>\(\)/)
  assert.match(page, /setIssued\(undefined\)/)
  assert.doesNotMatch(page, /localStorage.*secret|searchParams.*secret|sessionStorage.*secret/)
  assert.match(dialog, /organizationServiceAccounts\.keyDialog\.warning/)
})
