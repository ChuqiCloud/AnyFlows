import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { URL } from 'node:url'

import { routeFromHash } from '../src/app-route.ts'

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')

test('企业审批路由和导航入口保持独立', () => {
  assert.deepEqual(routeFromHash('#/console/organization-approvals'), {
    view: 'organization-approvals',
  })
  const navigation = read('../src/components/layout/app-navigation-model.ts')
  assert.match(navigation, /organizationApprovals/)
})

test('审批工作台覆盖权限、事实状态和并发冲突', () => {
  const page = read('../src/features/organization-approvals/organization-approval-page.tsx')
  const api = read('../src/features/organization-approvals/organization-approval-api.ts')

  assert.match(page, /organization\.approvals\.read/)
  assert.match(page, /organization\.approvals\.create/)
  assert.match(page, /organization\.approvals\.manage/)
  assert.match(page, /organization\.approvals\.decide/)
  assert.match(page, /timed_out/)
  assert.match(page, /organizationApprovals\.errors\.conflict/)
  assert.match(api, /listOrganizationApprovalTemplates/)
  assert.match(api, /listOrganizationApprovalRequests/)
  assert.match(api, /createOrganizationApprovalRequest/)
  assert.match(api, /decideOrganizationApprovalRequest/)
  assert.match(api, /invalidateQueries/)
  assert.match(api, /refetchInterval: enabled \? ORGANIZATION_APPROVAL_REQUEST_REFRESH_INTERVAL_MS : false/)
  assert.match(api, /refetchIntervalInBackground: false/)
})

test('审批中英文文案和生成客户端均存在', () => {
  const zh = JSON.parse(read('../src/i18n/locales/zh.json'))
  const en = JSON.parse(read('../src/i18n/locales/en.json'))
  for (const locale of [zh, en]) {
    assert.equal(typeof locale.organizationApprovals.title, 'string')
    assert.equal(typeof locale.organizationApprovals.status.pending, 'string')
    assert.equal(typeof locale.organizationApprovals.errors.conflict, 'string')
  }
  const sdk = read('../src/lib/api/generated/sdk.gen.ts')
  assert.match(sdk, /export const listOrganizationApprovalTemplates/)
  assert.match(sdk, /export const createOrganizationApprovalTemplate/)
  assert.match(sdk, /export const listOrganizationApprovalRequests/)
  assert.match(sdk, /export const decideOrganizationApprovalRequest/)
})
