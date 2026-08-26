import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const [httpSource, generatedSdk, generatedTypes] = await Promise.all([
  readFile(new URL('../../crates/af-http/src/organizations.rs', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/sdk.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/types.gen.ts', import.meta.url), 'utf8'),
])

test('企业 SSO 显式身份绑定 HTTP 与 OpenAPI、生成客户端保持同步', () => {
  const operation = openapi.paths['/api/organizations/{organization_id}/sso/identities/bind'].post
  assert.equal(operation.operationId, 'bindOrganizationSsoIdentity')
  assert.equal(
    operation.requestBody.content['application/json'].schema.$ref,
    '#/components/schemas/OrganizationSsoIdentityBindingRequest',
  )
  assert.equal(
    operation.responses['200'].content['application/json'].schema.$ref,
    '#/components/schemas/OrganizationSsoIdentityBindingResponse',
  )
  for (const status of ['400', '401', '403', '409', '422', '500']) {
    assert.ok(operation.responses[status], '缺少 ' + status + ' 错误响应')
  }
  assert.match(generatedSdk, /export const bindOrganizationSsoIdentity =/)
  assert.match(generatedTypes, /export type BindOrganizationSsoIdentityData =/)
})

test('身份绑定请求只接受闭合 step-up 材料，响应和错误不泄露敏感事实', () => {
  const requestSchema = openapi.components.schemas.OrganizationSsoIdentityBindingRequest
  const responseSchema = openapi.components.schemas.OrganizationSsoIdentityBindingResponse
  assert.deepEqual(Object.keys(requestSchema.properties).sort(), [
    'credential',
    'current_password',
    'method',
    'ticket',
    'totp_code',
  ])
  assert.deepEqual(requestSchema.required.sort(), ['method', 'ticket'])
  assert.equal(requestSchema.properties.method.pattern, '^(password|passkey)$')
  for (const field of ['ticket', 'current_password', 'totp_code']) {
    assert.equal(requestSchema.properties[field].writeOnly, true)
  }
  assert.deepEqual(requestSchema.properties.credential, {})
  assert.deepEqual(Object.keys(responseSchema.properties).sort(), [
    'binding_method',
    'identity_id',
    'membership_id',
    'organization_id',
    'status',
    'version',
  ])
  for (const field of ['subject', 'subject_digest', 'ticket', 'credential', 'current_password', 'totp_code']) {
    assert.equal(responseSchema.properties[field], undefined)
  }
  const errorCodes = openapi.components.schemas.ManagementError.properties.code.enum
  assert.ok(errorCodes.includes('organization_sso_identity_binding_rejected'))
  assert.ok(errorCodes.includes('organization_sso_identity_binding_conflict'))
  assert.match(httpSource, /OrganizationPermission::OverviewRead/)
  assert.match(httpSource, /exchange_ticket\(organization_id, ticket\)/)
  assert.match(httpSource, /bind_explicit_identity_with_step_up/)
  assert.doesNotMatch(httpSource, /subject_digest|invitation_email_fingerprint/)
})
