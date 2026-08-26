import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const [httpSource, generatedSdk, generatedTypes, generatedQuery] = await Promise.all([
  readFile(new URL('../../crates/af-http/src/organizations.rs', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/sdk.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/types.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/@tanstack/react-query.gen.ts', import.meta.url), 'utf8'),
])

const recoveryPaths = [
  ['/api/organizations/{organization_id}/sso/recovery/establish', 'establishOrganizationSsoRecovery'],
  ['/api/organizations/{organization_id}/sso/recovery/suspend-required', 'suspendOrganizationSsoRequired'],
]

test('Owner break-glass 恢复 HTTP 与 OpenAPI、生成客户端保持同步', () => {
  for (const [path, operationId] of recoveryPaths) {
    const operation = openapi.paths[path]?.post
    assert.ok(operation, `缺少 ${path}`)
    assert.equal(operation.operationId, operationId)
    assert.equal(
      operation.requestBody.content['application/json'].schema.$ref,
      '#/components/schemas/OrganizationSsoRecoveryRequest',
    )
    assert.equal(
      operation.responses['200'].content['application/json'].schema.$ref,
      '#/components/schemas/OrganizationSsoRecoveryResponse',
    )
    for (const status of ['400', '401', '403', '409', '422', '500', '503']) {
      assert.ok(operation.responses[status], `${path} 缺少 ${status} 错误响应`)
    }
  }
  assert.match(generatedSdk, /export const establishOrganizationSsoRecovery =/)
  assert.match(generatedSdk, /export const suspendOrganizationSsoRequired =/)
  assert.match(generatedTypes, /export type EstablishOrganizationSsoRecoveryData =/)
  assert.match(generatedTypes, /export type SuspendOrganizationSsoRequiredData =/)
  assert.match(generatedQuery, /establishOrganizationSsoRecoveryMutation/)
  assert.match(generatedQuery, /suspendOrganizationSsoRequiredMutation/)
})

test('恢复请求字段只写入，响应不暴露认证材料', () => {
  const requestSchema = openapi.components.schemas.OrganizationSsoRecoveryRequest
  const responseSchema = openapi.components.schemas.OrganizationSsoRecoveryResponse
  assert.deepEqual(Object.keys(requestSchema.properties).sort(), [
    'current_password',
    'expected_version',
    'totp_code',
  ])
  assert.deepEqual(requestSchema.required.sort(), ['current_password', 'expected_version'])
  for (const field of ['current_password', 'totp_code']) {
    assert.equal(requestSchema.properties[field].writeOnly, true)
  }
  assert.deepEqual(Object.keys(responseSchema.properties).sort(), [
    'policy',
    'recovery_ready',
    'version',
  ])
  for (const field of ['current_password', 'totp_code', 'recovery_material', 'secret', 'ticket']) {
    assert.equal(responseSchema.properties[field], undefined)
  }
  const errorCodes = openapi.components.schemas.ManagementError.properties.code.enum
  for (const code of [
    'organization_sso_recovery_rejected',
    'organization_sso_recovery_conflict',
    'organization_sso_recovery_unavailable',
  ]) {
    assert.ok(errorCodes.includes(code), `缺少错误码 ${code}`)
  }
  assert.match(httpSource, /OrganizationPermission::SsoRecovery/)
  assert.match(httpSource, /sso_recovery_service/)
})
