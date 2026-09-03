import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { URL } from 'node:url'

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')

test('企业审批通知使用生成客户端并保持稳定游标分页', () => {
  const api = read('../src/features/profile/profile-api.ts')
  const page = read('../src/features/profile/profile-page.tsx')
  const shell = read('../src/components/layout/app-shell.tsx')
  const sdk = read('../src/lib/api/generated/sdk.gen.ts')

  assert.match(api, /listOrganizationApprovalNotifications/)
  assert.match(api, /markOrganizationApprovalNotificationsRead/)
  assert.match(api, /getNextPageParam: \(page\) => page\.next_cursor/)
  assert.match(page, /OrganizationApprovalNotificationCard/)
  assert.match(page, /organizationApprovalNotifications\.markRead/)
  assert.match(page, /entry\.read_at/)
  assert.match(shell, /organizationApprovalNotificationsQuery/)
  assert.match(shell, /unreadCount = .*organizationApprovalNotificationsQuery/s)
  assert.match(sdk, /listOrganizationApprovalNotifications/)
  assert.match(sdk, /markOrganizationApprovalNotificationsRead/)
})

test('企业审批通知双语文案完整且不包含占位符', () => {
  for (const locale of ['en', 'zh']) {
    const source = read(`../src/i18n/locales/${locale}.json`)
    assert.match(source, /"organizationApprovalNotifications"\s*:/)
    assert.doesNotMatch(source, /"organizationApprovalNotifications"\s*:\s*\{[^}]*\?\s*\}/s)
  }
})
