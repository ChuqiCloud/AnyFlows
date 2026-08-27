import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const [httpSource, generatedSdk, generatedTypes, generatedQuery] = await Promise.all([
  readFile(new URL('../../crates/af-http/src/scim.rs', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/sdk.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/types.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/@tanstack/react-query.gen.ts', import.meta.url), 'utf8'),
])

test('企业 SCIM Groups HTTP、OpenAPI 与生成客户端保持同步', () => {
  const operations = [
    ['/scim/v2/{organization_id}/Groups', 'post', 'createScimGroup'],
    ['/scim/v2/{organization_id}/Groups/{resource_id}', 'put', 'replaceScimGroup'],
    ['/scim/v2/{organization_id}/Groups/{resource_id}', 'delete', 'deleteScimGroup'],
  ]
  for (const [path, method, operationId] of operations) {
    assert.equal(openapi.paths[path]?.[method]?.operationId, operationId)
  }
  assert.deepEqual(
    Object.keys(openapi.components.schemas.ScimGroupWriteRequest.properties).sort(),
    ['displayName', 'id', 'members', 'meta'],
  )
  assert.deepEqual(Object.keys(openapi.components.schemas.ScimGroupMember.properties), ['value'])
  assert.equal(openapi.paths['/scim/v2/{organization_id}/Groups/{resource_id}'].delete.parameters.find((item) => item.name === 'if-match').required, true)
  assert.match(generatedSdk, /export const createScimGroup =/)
  assert.match(generatedSdk, /export const replaceScimGroup =/)
  assert.match(generatedSdk, /export const deleteScimGroup =/)
  assert.match(generatedTypes, /export type ScimGroupWriteRequest =/)
  assert.match(generatedTypes, /'if-match': string;/)
  assert.match(generatedQuery, /createScimGroupMutation/)
  assert.match(generatedQuery, /replaceScimGroupMutation/)
  assert.match(generatedQuery, /deleteScimGroupMutation/)
})

test('Groups 只接受受限成员引用并保持删除版本边界', () => {
  assert.match(httpSource, /OrganizationScimGroupResource::new/)
  assert.match(httpSource, /parse_if_match_version/)
  assert.match(httpSource, /delete_group\(authentication\.token, resource_id, expected_version\)/)
  assert.match(httpSource, /OrganizationScimGroupSyncError::MemberUnavailable/)
  assert.doesNotMatch(httpSource, /serde_json::Value|JsonValue|merge_patch/)
})

