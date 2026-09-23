import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8')

test('企业开通申请的 HTTP、OpenAPI、生成客户端和前端入口保持同步', async () => {
  const [openapi, sdk, types, route, api, page, registrationPage, en, zh] = await Promise.all([
    read('../openapi/openapi.json').then(JSON.parse),
    read('../src/lib/api/generated/sdk.gen.ts'),
    read('../src/lib/api/generated/types.gen.ts'),
    read('../src/app-route.ts'),
    read('../src/features/organization-provisioning/organization-provisioning-api.ts'),
    read('../src/features/organization-provisioning/organization-provisioning-page.tsx'),
    read('../src/features/registration/registration-page.tsx'),
    read('../src/i18n/locales/en.json').then(JSON.parse),
    read('../src/i18n/locales/zh.json').then(JSON.parse),
  ])

  const paths = [
    ['/api/account/organization-provisioning', 'post', 'createOrganizationProvisioningRequest'],
    ['/api/account/organization-provisioning/active', 'get', 'getActiveOrganizationProvisioningRequest'],
    ['/api/account/organization-provisioning/{request_key}', 'get', 'getOrganizationProvisioningRequest'],
    ['/api/admin/organization-provisioning', 'get', 'listAdminOrganizationProvisioningRequests'],
    ['/api/admin/organization-provisioning/{request_key}', 'get', 'getAdminOrganizationProvisioningRequest'],
    ['/api/admin/organization-provisioning/{request_key}/decision', 'post', 'decideOrganizationProvisioningRequest'],
    ['/api/admin/organization-provisioning/{request_key}/materials', 'get', 'listAdminOrganizationProvisioningMaterials'],
    ['/api/admin/organization-provisioning/{request_key}/materials/{material_id}', 'get', 'downloadAdminOrganizationProvisioningMaterial'],
  ]
  for (const [path, method, operationId] of paths) {
    assert.equal(openapi.paths[path]?.[method]?.operationId, operationId)
    assert.match(sdk, new RegExp(`export const ${operationId}\\b`))
  }
  for (const schema of ['OrganizationProvisioningStatus', 'OrganizationProvisioningDecisionKind', 'OrganizationProvisioningCreateRequest', 'OrganizationProvisioningMaterial', 'OrganizationProvisioningEntitlement', 'OrganizationProvisioningDecisionRequest', 'OrganizationProvisioningRequest', 'OrganizationProvisioningRequestListResponse']) {
    assert.ok(openapi.components.schemas[schema])
    assert.match(types, new RegExp(`export type ${schema}\\b`))
  }
  assert.match(route, /organization-provisioning/)
  assert.match(page, /useActiveOrganizationProvisioning/)
  assert.match(page, /useAdminOrganizationProvisioning/)
  assert.match(page, /ProvisioningFlow current="apply"/)
  assert.match(page, /ProvisioningFlow current="review"/)
  assert.match(page, /type AdminProvisioningView = 'review' \| 'apply'/)
  assert.match(page, /role="tablist"/)
  assert.match(page, /adminView === 'review' \? <AdminProvisioningWorkspace \/> : <ProvisioningEligibility><ApplicantProvisioningWorkspace \/><\/ProvisioningEligibility>/)
  assert.match(page, /const shouldLoadRequest = requestKey !== undefined && !activeQuery\.isPending && activeQuery\.data === null/)
  assert.match(page, /shouldLoadRequest && requestQuery\.isPending/)
  assert.match(page, /const \[submittedRequest, setSubmittedRequest\]/)
  assert.match(page, /setSubmittedRequest\(result\)/)
  assert.match(page, /type="file"/)
  assert.match(page, /MAX_TOTAL_MATERIAL_BYTES/)
  assert.match(page, /useAdminOrganizationProvisioningMaterials/)
  assert.match(page, /downloadAdminOrganizationProvisioningMaterial/)
  assert.match(api, /bodySerializer: null/)
  assert.match(api, /headers: \{ 'Content-Type': null \}/)
  assert.match(sdk, /formDataBodySerializer/)
  assert.match(registrationPage, /registration\.enterprise\.steps/)
  assert.equal(typeof en.organizationProvisioning.title, 'string')
  assert.equal(typeof zh.organizationProvisioning.title, 'string')
  for (const locale of [en, zh]) {
    assert.deepEqual(Object.keys(locale.organizationProvisioning.flow), ['apply', 'review', 'setup', 'operate'])
    assert.deepEqual(Object.keys(locale.organizationProvisioning.admin.views), ['label', 'review', 'apply'])
    assert.deepEqual(Object.keys(locale.registration.enterprise.steps), ['account', 'apply', 'setup'])
  }
  assert.equal(typeof en.organizationConsole.empty.applyProvisioning, 'string')
  assert.equal(typeof zh.organizationConsole.empty.applyProvisioning, 'string')
})
