import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const [httpSource, serviceSource, generatedTypes] = await Promise.all([
  readFile(new URL('../../crates/af-http/src/organizations.rs', import.meta.url), 'utf8'),
  readFile(new URL('../../crates/af-admin/src/organization_sso_domain.rs', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/types.gen.ts', import.meta.url), 'utf8'),
])

test('企业 SSO 域名 HTTP 路径与生成客户端契约一致', () => {
  const operations = [
    ['/api/organizations/{organization_id}/sso/domains', 'get', 'listOrganizationSsoDomains'],
    ['/api/organizations/{organization_id}/sso/domains', 'post', 'createOrganizationSsoDomain'],
    ['/api/organizations/{organization_id}/sso/domains/{domain_id}/verify', 'post', 'verifyOrganizationSsoDomain'],
  ]
  for (const [path, method, operationId] of operations) {
    assert.equal(openapi.paths[path][method].operationId, operationId)
  }
  assert.match(generatedTypes, /export type OrganizationSsoDomainCreated = \{[\s\S]*txt_record_value: string;/)
  assert.match(generatedTypes, /export type OrganizationSsoDomainVerifyRequestWritable = \{[\s\S]*proof_token: string;/)
})

test('域名响应不暴露证明摘要，创建响应只返回一次性 TXT 值', () => {
  const domainSchema = openapi.components.schemas.OrganizationSsoDomain
  const createdSchema = openapi.components.schemas.OrganizationSsoDomainCreated
  const verifySchema = openapi.components.schemas.OrganizationSsoDomainVerifyRequest

  assert.equal(domainSchema.properties.proof_digest, undefined)
  assert.equal(domainSchema.properties.proof_token, undefined)
  assert.equal(createdSchema.properties.txt_record_value.writeOnly, undefined)
  assert.ok(createdSchema.required.includes('txt_record_value'))
  assert.equal(verifySchema.properties.proof_token.writeOnly, true)
  assert.equal(verifySchema.properties.dns_answers, undefined)
  assert.match(httpSource, /struct OrganizationSsoDomainCreatedResponse[\s\S]*txt_record_value: String/)
  assert.doesNotMatch(httpSource, /txt_record_value[\s\S]{0,120}write_only/)
})

test('服务端验证按企业范围读取并使用受控 DNS 解析器', () => {
  assert.match(serviceSource, /list_domains\(context\.organization_id\(\)\)/)
  assert.match(serviceSource, /domain\(context\.organization_id\(\), domain_id\)/)
  assert.match(serviceSource, /lookup_txt\(&proof\)/)
  assert.match(serviceSource, /MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS/)
  assert.doesNotMatch(serviceSource, /client_dns|dns_answers|dns_result/)
})
