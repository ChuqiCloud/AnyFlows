import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8')

test('企业认证的 HTTP、OpenAPI、生成客户端和前端入口保持同步', async () => {
  const [openapi, sdk, types, route, page, settings, en, zh] = await Promise.all([
    read('../openapi/openapi.json').then(JSON.parse),
    read('../src/lib/api/generated/sdk.gen.ts'),
    read('../src/lib/api/generated/types.gen.ts'),
    read('../src/app-route.ts'),
    read('../src/features/organization-verification/organization-verification-page.tsx'),
    read('../src/features/organization-console/organization-settings-page.tsx'),
    read('../src/i18n/locales/en.json').then(JSON.parse),
    read('../src/i18n/locales/zh.json').then(JSON.parse),
  ])

  const paths = [
    ['/api/organizations/{organization_id}/verification', 'get', 'getOrganizationVerification'],
    ['/api/organizations/{organization_id}/verification', 'post', 'submitOrganizationVerification'],
    ['/api/admin/organization-verifications', 'get', 'listAdminOrganizationVerifications'],
    ['/api/admin/organization-verifications/{case_id}', 'get', 'getAdminOrganizationVerification'],
    ['/api/admin/organization-verifications/{case_id}/decision', 'post', 'decideOrganizationVerification'],
    ['/api/admin/organization-verifications/{case_id}/materials/{material_id}', 'get', 'downloadAdminOrganizationVerificationMaterial'],
  ]
  for (const [path, method, operationId] of paths) {
    assert.equal(openapi.paths[path]?.[method]?.operationId, operationId)
    assert.match(sdk, new RegExp(`export const ${operationId}\\b`))
  }
  for (const schema of ['OrganizationVerificationStatus', 'OrganizationVerificationMultipartSubmitRequest', 'OrganizationVerificationResponse', 'OrganizationVerificationDecisionRequest', 'OrganizationVerificationListResponse']) {
    assert.ok(openapi.components.schemas[schema])
    assert.match(types, new RegExp(`export type ${schema}\\b`))
  }
  assert.match(route, /organization-verification/)
  assert.match(page, /useOrganizationVerification/)
  assert.match(page, /useAdminOrganizationVerifications/)
  assert.match(page, /type="file"/)
  assert.match(page, /downloadAdminOrganizationVerificationMaterial/)
  assert.match(page, /VerificationProcess/)
  assert.match(settings, /organization-verification/)
  assert.equal(typeof en.organizationVerification.title, 'string')
  assert.equal(typeof zh.organizationVerification.title, 'string')
  for (const locale of [en, zh]) {
    assert.equal(typeof locale.organizationVerification.process.title, 'string')
    assert.equal(typeof locale.organizationVerification.process.steps.submit.body, 'string')
    assert.equal(typeof locale.organizationVerification.process.steps.boundary.body, 'string')
  }
})
