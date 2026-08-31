import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

test('公开企业 SSO 发现契约只返回通用可用性', () => {
  const openapi = JSON.parse(readFileSync(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
  const source = readFileSync(new URL('../src/features/auth/login-page.tsx', import.meta.url), 'utf8')
  const apiSource = readFileSync(new URL('../src/features/auth/organization-sso-discovery-api.ts', import.meta.url), 'utf8')
  const zh = JSON.parse(readFileSync(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))
  const en = JSON.parse(readFileSync(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8'))
  const operation = openapi.paths['/api/auth/organization-sso/discovery'].post

  assert.equal(operation.operationId, 'discoverOrganizationSso')
  assert.deepEqual(Object.keys(openapi.components.schemas.OrganizationSsoDiscoveryResponse.properties), ['available'])
  assert.deepEqual(Object.keys(openapi.components.schemas.OrganizationSsoDiscoveryRequest.properties), ['identifier'])
  assert.match(source, /organizationSsoMutation/)
  assert.match(source, /organizationSsoStatus/)
  assert.match(source, /organizationSso\.\$\{organizationSsoStatus\}/)
  assert.equal(typeof zh.auth.login.organizationSso.available, 'string')
  assert.equal(typeof en.auth.login.organizationSso.unavailable, 'string')
  assert.match(apiSource, /discoverOrganizationSso as discoverOrganizationSsoRequest/)
  assert.match(apiSource, /body: \{ identifier \}/)
  assert.doesNotMatch(JSON.stringify(openapi.components.schemas.OrganizationSsoDiscoveryResponse), /organization_id|provider_key|authorization_url/)
})
