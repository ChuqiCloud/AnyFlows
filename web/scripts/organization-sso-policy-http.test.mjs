import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const [httpSource, apiSource, policySource, generatedSdk, generatedTypes, generatedQuery] = await Promise.all([
  readFile(new URL('../../crates/af-http/src/organizations.rs', import.meta.url), 'utf8'),
  readFile(new URL('../src/features/organization-sso/organization-sso-api.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/features/organization-sso/organization-sso-policy.tsx', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/sdk.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/types.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/@tanstack/react-query.gen.ts', import.meta.url), 'utf8'),
])

test('企业 SSO 策略 HTTP、OpenAPI 与生成客户端保持同步', () => {
  const path = '/api/organizations/{organization_id}/sso/policy'
  const getOperation = openapi.paths[path]?.get
  const updateOperation = openapi.paths[path]?.put

  assert.equal(getOperation?.operationId, 'getOrganizationSsoPolicy')
  assert.equal(
    getOperation?.responses['200'].content['application/json'].schema.$ref,
    '#/components/schemas/OrganizationSsoPolicyResponse',
  )
  for (const status of ['400', '401', '403', '500', '503']) {
    assert.ok(getOperation?.responses[status], `GET 缺少 ${status} 错误响应`)
  }
  assert.equal(updateOperation?.operationId, 'updateOrganizationSsoPolicy')
  assert.equal(
    updateOperation?.requestBody.content['application/json'].schema.$ref,
    '#/components/schemas/OrganizationSsoPolicyRequest',
  )
  assert.equal(
    updateOperation?.responses['200'].content['application/json'].schema.$ref,
    '#/components/schemas/OrganizationSsoPolicyResponse',
  )
  for (const status of ['400', '401', '403', '409', '500', '503']) {
    assert.ok(updateOperation?.responses[status], `PUT 缺少 ${status} 错误响应`)
  }
  assert.match(generatedSdk, /export const getOrganizationSsoPolicy =/)
  assert.match(generatedSdk, /export const updateOrganizationSsoPolicy =/)
  assert.match(generatedTypes, /export type OrganizationSsoPolicyResponse =/)
  assert.match(generatedTypes, /export type OrganizationSsoPolicyRequest =/)
  assert.match(generatedQuery, /getOrganizationSsoPolicyOptions/)
  assert.match(generatedQuery, /updateOrganizationSsoPolicyMutation/)
})

test('策略响应只投影强制模式、恢复就绪和版本', () => {
  const requestSchema = openapi.components.schemas.OrganizationSsoPolicyRequest
  const responseSchema = openapi.components.schemas.OrganizationSsoPolicyResponse

  assert.deepEqual(Object.keys(requestSchema.properties).sort(), ['expected_version', 'policy'])
  assert.deepEqual(requestSchema.required.sort(), ['expected_version', 'policy'])
  assert.deepEqual(requestSchema.properties.policy.enum, ['optional', 'required'])
  assert.deepEqual(Object.keys(responseSchema.properties).sort(), ['policy', 'recovery_ready', 'version'])
  for (const field of ['current_password', 'totp_code', 'recovery_material', 'secret', 'ticket']) {
    assert.equal(responseSchema.properties[field], undefined)
  }
  assert.match(httpSource, /OrganizationPermission::SsoRead/)
  assert.match(httpSource, /OrganizationPermission::SsoRecovery/)
  assert.match(httpSource, /sso_policy_service/)
  assert.match(httpSource, /no_store_json\(OrganizationSsoPolicyResponse/)
})

test('策略前端按权限读取并且不缓存 Owner 的敏感恢复字段', () => {
  assert.match(apiSource, /useOrganizationSsoPolicy/)
  assert.match(apiSource, /getOrganizationSsoPolicy/)
  assert.match(apiSource, /updateOrganizationSsoPolicy/)
  assert.match(policySource, /OrganizationSsoPolicyResponse/)
  assert.match(policySource, /useEstablishOrganizationSsoRecovery/)
  assert.match(policySource, /useSuspendOrganizationSsoRequired/)
  assert.match(policySource, /setPassword\(''\)/)
  assert.match(policySource, /setTotp\(''\)/)
  assert.doesNotMatch(policySource, /localStorage|sessionStorage|queryClient/)
})
