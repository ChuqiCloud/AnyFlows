import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'

test('企业活动路由进入独立工作空间视图', () => {
  assert.deepEqual(routeFromHash('#/console/organization-activity'), { view: 'organization-activity' })
  assert.deepEqual(routeFromHash('#/console/organization-activity?source=nav'), { view: 'organization-activity' })
})

test('页面按企业权限裁剪调用范围与审计页签', async () => {
  const [pageSource, workspaceSource] = await Promise.all([
    readFile(new URL('../src/features/organization-activity/organization-activity-page.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-console/organization-workspace-context.tsx', import.meta.url), 'utf8'),
  ])
  assert.match(pageSource, /organization\.usage_logs\.read_all/)
  assert.match(pageSource, /organization\.usage_logs\.read_self/)
  assert.match(pageSource, /organization\.audit\.read/)
  assert.match(pageSource, /overview\.role === 'viewer'/)
  assert.match(pageSource, /\['summary', 'self'\]/)
  assert.match(pageSource, /useOrganizationWorkspace/)
  assert.match(pageSource, /key=\{workspace\.selectedId\}/)
  assert.match(workspaceSource, /clearOrganizationWorkspaceCache/)
  assert.match(pageSource, /hidden=\{activeTab !== 'usage'\}/)
  assert.match(pageSource, /hidden=\{activeTab !== 'audit'\}/)
})

test('企业日志查询绑定组织、范围、游标和服务端审计筛选', async () => {
  const apiSource = await readFile(new URL('../src/features/organization-activity/organization-activity-api.ts', import.meta.url), 'utf8')
  assert.match(apiSource, /\['organization', organizationId, 'activity'/)
  assert.match(apiSource, /listOrganizationUsageLogs/)
  assert.match(apiSource, /query: \{ before, limit, scope \}/)
  assert.match(apiSource, /listOrganizationAuditLogs/)
  assert.match(apiSource, /actor_user_id: filters\.actorUserId/)
  assert.match(apiSource, /operation: filters\.operation/)
  assert.match(apiSource, /target_type: filters\.targetType/)
  assert.match(apiSource, /outcome: filters\.outcome/)
})

test('企业调用表复用高密度明细且不开放平台管理动作', async () => {
  const [usageSource, tableSource, detailSource] = await Promise.all([
    readFile(new URL('../src/features/organization-activity/organization-usage-panel.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/usage-logs/usage-log-table.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/usage-logs/usage-log-details-sheet.tsx', import.meta.url), 'utf8'),
  ])
  assert.match(usageSource, /<UsageLogTable/)
  assert.match(usageSource, /admin=\{false\}/)
  assert.match(usageSource, /showPrincipal=\{scope === 'organization'\}/)
  assert.match(tableSource, /showPrincipal = admin/)
  assert.match(detailSource, /action: admin && log\.request_id/)
})

test('企业活动 OpenAPI、平台代理接口与中英文入口保持同步', async () => {
  const [openapi, zh, en] = await Promise.all([
    readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8').then(JSON.parse),
  ])
  assert.ok(openapi.paths['/api/organizations/{organization_id}/usage-logs'])
  assert.ok(openapi.paths['/api/organizations/{organization_id}/audit-logs'])
  assert.ok(openapi.paths['/api/platform/organizations/{organization_id}/usage-logs'])
  assert.ok(openapi.paths['/api/platform/organizations/{organization_id}/audit-logs'])
  assert.ok(openapi.components.schemas.OrganizationUsageLogListResponse)
  assert.ok(openapi.components.schemas.OrganizationAuditLogResponse)
  for (const locale of [zh, en]) {
    assert.equal(typeof locale.nav.organizationActivity, 'string')
    assert.equal(typeof locale.organizationActivity.tabs.usage, 'string')
    assert.equal(typeof locale.organizationActivity.audit.filters.operation, 'string')
    assert.equal(typeof locale.organizationActivity.errors.auditBody, 'string')
  }
})
