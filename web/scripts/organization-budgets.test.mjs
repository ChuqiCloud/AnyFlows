import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildOrganizationBudgetSchema,
  defaultOrganizationBudgetValues,
  fixedPeriodSeconds,
  latestScopeEffectiveFrom,
  organizationBudgetScopeKey,
  toOrganizationBudgetRequest,
} from '../src/features/organization-budgets/organization-budget-form-model.ts'

const messages = {
  scope: 'scope',
  fixedPeriod: 'fixed',
  quotaLimit: 'quota',
  effectiveFrom: 'effective',
  effectiveConflict: 'conflict',
}

const priorTeamPolicy = {
  id: 12,
  organization_id: 5,
  scope: { kind: 'team', team_id: 9 },
  period: { kind: 'utc_month' },
  quota_limit: 1000,
  effective_from: 1_776_592_800,
  effective_until: null,
  version: 2,
}

function schema(policies = []) {
  return buildOrganizationBudgetSchema({
    activeTeamIds: new Set([9]),
    activeMembershipIds: new Set([21]),
    policies,
  }, messages)
}

test('企业预算路由进入独立工作空间视图', () => {
  assert.deepEqual(routeFromHash('#/console/organization-budgets'), { view: 'organization-budgets' })
  assert.deepEqual(routeFromHash('#/console/organization-budgets?source=nav'), { view: 'organization-budgets' })
})

test('固定周期只接受 60 秒到 366 天的精确整数', () => {
  assert.equal(fixedPeriodSeconds('59', 'seconds'), undefined)
  assert.equal(fixedPeriodSeconds('60', 'seconds'), 60)
  assert.equal(fixedPeriodSeconds('90', 'minutes'), 5_400)
  assert.equal(fixedPeriodSeconds('366', 'days'), 31_622_400)
  assert.equal(fixedPeriodSeconds('367', 'days'), undefined)
  assert.equal(fixedPeriodSeconds('1.5', 'hours'), undefined)
})

test('有限额度与 null 不限额生成闭合预算请求', () => {
  const effectiveFrom = '2026-08-18T09:30'
  const base = {
    ...defaultOrganizationBudgetValues(new Date('2026-08-17T09:00:00')),
    effectiveFrom,
    quotaLimit: '500000',
  }
  assert.equal(schema().safeParse(base).success, true)
  assert.deepEqual(toOrganizationBudgetRequest(base), {
    scope: { kind: 'organization' },
    period: { kind: 'utc_month' },
    quota_limit: 500000,
    effective_from: Math.floor(new Date(effectiveFrom).getTime() / 1_000),
  })
  assert.deepEqual(toOrganizationBudgetRequest({ ...base, unlimited: true, quotaLimit: '' }), {
    scope: { kind: 'organization' },
    period: { kind: 'utc_month' },
    quota_limit: null,
    effective_from: Math.floor(new Date(effectiveFrom).getTime() / 1_000),
  })
})

test('团队和成员策略必须选择当前企业的活动目录项', () => {
  const values = {
    ...defaultOrganizationBudgetValues(new Date('2026-08-17T09:00:00')),
    scopeKind: 'team',
    teamId: '9',
    quotaLimit: '1000',
    effectiveFrom: '2026-08-18T10:00',
  }
  assert.equal(schema().safeParse(values).success, true)
  assert.deepEqual(toOrganizationBudgetRequest(values).scope, { kind: 'team', team_id: 9 })
  assert.equal(schema().safeParse({ ...values, teamId: '10' }).success, false)
  assert.equal(schema().safeParse({
    ...values,
    scopeKind: 'member',
    teamId: '',
    membershipId: '21',
  }).success, true)
})

test('同一作用域的新版本必须晚于上一版本生效', () => {
  const conflictTime = new Date(priorTeamPolicy.effective_from * 1_000)
  const localConflictTime = new Date(conflictTime.getTime() - conflictTime.getTimezoneOffset() * 60_000)
    .toISOString().slice(0, 16)
  const values = {
    ...defaultOrganizationBudgetValues(),
    scopeKind: 'team',
    teamId: '9',
    quotaLimit: '2000',
    effectiveFrom: localConflictTime,
  }
  assert.equal(latestScopeEffectiveFrom([priorTeamPolicy], values), priorTeamPolicy.effective_from)
  assert.equal(schema([priorTeamPolicy]).safeParse(values).success, false)
  assert.equal(organizationBudgetScopeKey(priorTeamPolicy.scope), 'team:9')
})

test('页面按权限门控预算查询并在切换时释放组织缓存', async () => {
  const [pageSource, apiSource, workspaceSource] = await Promise.all([
    readFile(new URL('../src/features/organization-budgets/organization-budget-page.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-budgets/organization-budget-api.ts', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-console/organization-workspace-context.tsx', import.meta.url), 'utf8'),
  ])
  assert.match(pageSource, /permissions\.includes\(BUDGET_PERMISSION\)/)
  assert.match(pageSource, /useOrganizationBudgets\(organizationId, canManage\)/)
  assert.match(pageSource, /useOrganizationWorkspace/)
  assert.match(pageSource, /key=\{workspace\.selectedId\}/)
  assert.match(apiSource, /await queryClient\.cancelQueries/)
  assert.match(apiSource, /queryClient\.removeQueries/)
  assert.match(workspaceSource, /clearOrganizationWorkspaceCache/)
})

test('企业预算中英文文案与移动端无宽表格实现完整', async () => {
  const [zh, en, overviewSource] = await Promise.all([
    readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/features/organization-budgets/organization-budget-overview.tsx', import.meta.url), 'utf8'),
  ])
  for (const locale of [zh, en]) {
    assert.equal(typeof locale.nav.organizationBudgets, 'string')
    assert.equal(typeof locale.organizationBudgets.form.title, 'string')
    assert.equal(typeof locale.organizationBudgets.errors.mutation.budget_outcome_unknown.body, 'string')
  }
  assert.doesNotMatch(overviewSource, /<table|overflow-x-auto/)
  assert.match(overviewSource, /sm:grid-cols/)
})
