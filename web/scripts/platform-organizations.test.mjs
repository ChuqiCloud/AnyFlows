import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import { isValidPlatformOrganizationReason } from '../src/features/platform-organizations/platform-organization-model.ts'

const read = async (path) => readFile(new URL(path, import.meta.url), 'utf8')

test('平台企业目录路由保持独立管理员入口', () => {
  assert.deepEqual(routeFromHash('#/console/platform-organizations'), { view: 'platform-organizations' })
  assert.deepEqual(routeFromHash('#/console/platform-organizations?ignored=true'), { view: 'platform-organizations' })
})

test('跨企业理由遵守服务端 UTF-8 与空白边界', () => {
  assert.equal(isValidPlatformOrganizationReason('核查企业归属'), true)
  assert.equal(isValidPlatformOrganizationReason(''), false)
  assert.equal(isValidPlatformOrganizationReason(' leading'), false)
  assert.equal(isValidPlatformOrganizationReason('line\nbreak'), false)
  assert.equal(isValidPlatformOrganizationReason('企'.repeat(86)), false)
})

test('企业目录与用户归属请求只通过审计 Header 传递理由', async () => {
  const source = await read('../src/features/platform-organizations/platform-organization-api.ts')
  assert.match(source, /'x-anyflows-platform-reason': access\.reason/)
  assert.doesNotMatch(source, /localStorage|sessionStorage/)
  assert.match(source, /gcTime: 0/)
})

test('平台企业响应固定为不含资金密钥与认证材料的安全字段', async () => {
  const openapi = JSON.parse(await read('../openapi/openapi.json'))
  const directory = openapi.paths['/api/platform/organizations'].get
  const memberships = openapi.paths['/api/platform/users/{user_id}/organizations'].get
  for (const operation of [directory, memberships]) {
    const reason = operation.parameters.find((parameter) => parameter.name === 'x-anyflows-platform-reason')
    assert.equal(reason.in, 'header')
    assert.equal(reason.required, true)
  }
  assert.deepEqual(
    Object.keys(openapi.components.schemas.PlatformOrganizationSummaryResponse.properties),
    [
      'id', 'name', 'slug', 'organization_status', 'entitlement_status',
      'owner_membership_id', 'owner_user_id', 'owner_username',
      'active_member_count', 'seat_limit', 'valid_until', 'grace_until', 'version',
    ],
  )
  assert.deepEqual(
    Object.keys(openapi.components.schemas.PlatformUserOrganizationSummaryResponse.properties),
    [
      'organization_id', 'organization_name', 'organization_slug',
      'organization_status', 'entitlement_status', 'membership_id',
      'role', 'membership_status', 'joined_at', 'version',
    ],
  )
})

test('平台企业目录具备中英文完整文案', async () => {
  for (const locale of ['zh', 'en']) {
    const messages = JSON.parse(await read(`../src/i18n/locales/${locale}.json`))
    assert.equal(typeof messages.nav.platformOrganizations, 'string')
    assert.equal(typeof messages.platformOrganizations.title, 'string')
    assert.equal(typeof messages.users.actions.organizations, 'string')
    assert.equal(typeof messages.users.organizations.reasonLabel, 'string')
  }
})
