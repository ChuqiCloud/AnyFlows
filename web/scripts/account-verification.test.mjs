import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8')

test('账号认证上传使用 multipart，材料按需读取并释放预览资源', async () => {
  const [api, attachment, records] = await Promise.all([
    read('../src/features/account-verification/account-verification-api.ts'),
    read('../src/features/account-verification/verification-attachment.tsx'),
    read('../src/features/account-verification/verification-records.tsx'),
  ])
  assert.match(api, /bodySerializer: null/)
  assert.match(api, /headers: \{ 'Content-Type': null \}/)
  assert.match(api, /data\.append\(`file:\$\{index\}`/)
  assert.match(api, /parseAs: 'blob'/)
  assert.match(api, /limit: '25'/)
  assert.match(api, /getNextPageParam/)
  assert.match(attachment, /URL\.revokeObjectURL/)
  assert.match(attachment, /controller\.signal/)
  assert.match(attachment, /sandbox=""/)
  assert.match(records, /can_apply_for_organization/)
  assert.match(records, /ProvisioningHistory/)
  assert.doesNotMatch(api + attachment, /localStorage|sessionStorage|indexedDB/)
})

test('认证接口契约要求管理会话，生成客户端包含新操作', async () => {
  const [document, sdk] = await Promise.all([
    read('../openapi/openapi.json').then(JSON.parse),
    read('../src/lib/api/generated/sdk.gen.ts'),
  ])
  for (const path of [
    '/api/account/verifications',
    '/api/account/verifications/eligibility',
    '/api/account/verifications/{case_id}',
    '/api/account/verifications/{case_id}/materials/{material_id}',
    '/api/admin/account-verifications',
    '/api/admin/account-verifications/{case_id}',
    '/api/admin/account-verifications/{case_id}/materials/{material_id}',
    '/api/admin/account-verifications/{case_id}/decision',
  ]) {
    assert.ok(document.paths[path], path)
    for (const operation of Object.values(document.paths[path])) {
      assert.deepEqual(operation.security, [{ bearerAuth: [] }], path)
      assert.match(sdk, new RegExp(`export const ${operation.operationId}\\b`))
    }
  }
})

test('认证与授权文案完整且记录状态为可显示字符串', async () => {
  const locales = await Promise.all(['en', 'zh'].map((locale) => read(`../src/i18n/locales/${locale}.json`).then(JSON.parse)))
  for (const locale of locales) {
    for (const key of ['title', 'enterpriseGranted', 'applyWorkspace', 'legacyMissing', 'legacyHistoryHint']) assert.equal(typeof locale.verificationCenter[key], 'string', key)
    for (const status of [1, 3, 4, 5]) assert.equal(typeof locale.verificationCenter.status[status], 'string')
    for (const status of ['pending', 'approved', 'rejected']) assert.equal(typeof locale.organizationProvisioning.status[status], 'string')
    for (const key of ['sso_enabled', 'custom_roles_enabled', 'disableSsoHint', 'reason']) assert.equal(typeof locale.enterpriseEntitlement[key], 'string', key)
  }
})
