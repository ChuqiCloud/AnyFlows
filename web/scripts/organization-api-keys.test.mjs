import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildOrganizationApiKeyFormSchema,
  defaultOrganizationApiKeyValues,
  toOrganizationApiKeyRequest,
} from '../src/features/organization-api-keys/organization-api-key-form-model.ts'

const messages = {
  invalidField: 'field',
  invalidModels: 'models',
  invalidIps: 'ips',
  invalidDate: 'date',
}

test('企业 API Key 路由进入独立工作空间视图', () => {
  assert.deepEqual(routeFromHash('#/console/organization-api-keys'), { view: 'organization-api-keys' })
  assert.deepEqual(routeFromHash('#/console/organization-api-keys?source=nav'), { view: 'organization-api-keys' })
})

test('企业 Key 表单复用基础策略并闭合多窗口限制', () => {
  const values = {
    ...defaultOrganizationApiKeyValues(),
    name: 'production',
    remainQuota: 500000,
    unlimitedQuota: false,
    modelLimits: ['gpt-5.5'],
    allowIps: ['192.0.2.0/24'],
    rateLimit5h: '100000',
    rateLimit1d: '',
    rateLimit7d: '700000',
    maxRequests: '1200',
  }
  assert.equal(buildOrganizationApiKeyFormSchema(messages).safeParse(values).success, true)
  assert.deepEqual(toOrganizationApiKeyRequest(values), {
    name: 'production',
    status: 'enabled',
    remain_quota: 500000,
    unlimited_quota: false,
    expired_at: null,
    model_limits: ['gpt-5.5'],
    allow_ips: ['192.0.2.0/24'],
    rate_limit_5h: 100000,
    rate_limit_1d: null,
    rate_limit_7d: 700000,
    max_requests: 1200,
  })
  assert.equal(buildOrganizationApiKeyFormSchema(messages).safeParse({ ...values, rateLimit5h: '+1' }).success, false)
  assert.equal(buildOrganizationApiKeyFormSchema(messages).safeParse({ ...values, maxRequests: '1.5' }).success, false)
})

test('页面按服务端权限分离本人管理和管理员脱敏目录', async () => {
  const [pageSource, apiSource, tableSource] = await Promise.all([
    readFile(new URL('../src/features/organization-api-keys/organization-api-key-page.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-api-keys/organization-api-key-api.ts', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-api-keys/organization-api-key-table.tsx', import.meta.url), 'utf8'),
  ])
  assert.match(pageSource, /organization\.keys\.manage_self/)
  assert.match(pageSource, /organization\.keys\.read_all/)
  assert.match(pageSource, /clearOrganizationWorkspaceCache/)
  assert.match(pageSource, /key=\{selectedId\}/)
  assert.match(apiSource, /listOrganizationTokens/)
  assert.match(apiSource, /listOrganizationTokenDirectory/)
  assert.match(apiSource, /disableOwnOrganizationToken/)
  assert.match(apiSource, /disableOrganizationToken/)
  assert.match(tableSource, /token\.key_prefix/)
  assert.doesNotMatch(tableSource, /token\.api_key/)
})

test('企业 Key 明文只交给一次性对话框且不误导为试炼场凭据', async () => {
  const [pageSource, formSource] = await Promise.all([
    readFile(new URL('../src/features/organization-api-keys/organization-api-key-page.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-api-keys/organization-api-key-form.tsx', import.meta.url), 'utf8'),
  ])
  assert.match(pageSource, /IssuedApiKeyDialog issued=\{issued\} showPlayground=\{false\}/)
  assert.match(formSource, /onIssued\(issued\)/)
  assert.doesNotMatch(formSource, /issued\.api_key/)
})

test('企业 Key OpenAPI 与中英文入口保持同步', async () => {
  const [openapi, zh, en] = await Promise.all([
    readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8').then(JSON.parse),
  ])
  assert.ok(openapi.paths['/api/organizations/{organization_id}/tokens'])
  assert.ok(openapi.paths['/api/organizations/{organization_id}/tokens/directory'])
  assert.ok(openapi.paths['/api/organizations/{organization_id}/tokens/{token_id}/disable'])
  assert.ok(openapi.components.schemas.OrganizationTokenResponse)
  assert.ok(openapi.components.schemas.ManagementError.properties.code.enum.includes('token_outcome_unknown'))
  for (const locale of [zh, en]) {
    assert.equal(typeof locale.nav.organizationApiKeys, 'string')
    assert.equal(typeof locale.organizationApiKeys.views.directory, 'string')
    assert.equal(typeof locale.organizationApiKeys.form.outcomeUnknown, 'string')
    assert.equal(typeof locale.organizationApiKeys.disable.directoryDescription, 'string')
  }
})
