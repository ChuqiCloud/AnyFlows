import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const [httpSource, serviceSource, generatedSdk, generatedTypes] = await Promise.all([
  readFile(new URL('../../crates/af-http/src/organizations.rs', import.meta.url), 'utf8'),
  readFile(new URL('../../crates/af-admin/src/organization_sso_login.rs', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/sdk.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/types.gen.ts', import.meta.url), 'utf8'),
])

test('企业 SSO 登录事务启动 HTTP 与 OpenAPI、生成客户端保持同步', () => {
  const operation = openapi.paths['/api/organizations/{organization_id}/sso/login/start'].post
  assert.equal(operation.operationId, 'startOrganizationSsoLogin')
  assert.equal(
    operation.requestBody.content['application/json'].schema.$ref,
    '#/components/schemas/OrganizationSsoLoginStartRequest',
  )
  assert.equal(
    operation.responses['200'].content['application/json'].schema.$ref,
    '#/components/schemas/OrganizationSsoLoginStartResponse',
  )
  assert.match(generatedSdk, /export const startOrganizationSsoLogin =/)
  assert.match(generatedTypes, /export type StartOrganizationSsoLoginData =/)
})

test('登录事务响应只包含脱敏摘要、Provider 版本绑定与受控浏览器动作', () => {
  const requestSchema = openapi.components.schemas.OrganizationSsoLoginStartRequest
  const responseSchema = openapi.components.schemas.OrganizationSsoLoginStartResponse
  assert.deepEqual(Object.keys(requestSchema.properties).sort(), ['provider_key', 'request_digest'])
  assert.deepEqual(
    Object.keys(responseSchema.properties).sort(),
    ['authorization_url', 'expires_at', 'protocol', 'provider_version', 'request_digest', 'saml_authn_request', 'state_digest'],
  )
  for (const sensitiveField of ['state', 'pkce_verifier', 'nonce', 'authorization_code', 'saml_response']) {
    assert.equal(responseSchema.properties[sensitiveField], undefined)
  }
  assert.match(httpSource, /OrganizationSsoLoginTransactionService/)
  assert.match(httpSource, /OrganizationPermission::SsoRead/)
  assert.match(httpSource, /OrganizationEntitlementStatus::Active/)
  assert.match(httpSource, /no_store_json\(OrganizationSsoLoginStartResponse/)
  assert.match(serviceSource, /OrganizationSsoLoginTransactionStart/)
  assert.doesNotMatch(httpSource, /pkce_verifier|authorization_code|saml_response/)
})
