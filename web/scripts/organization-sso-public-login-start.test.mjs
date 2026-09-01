import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const [openapi, httpSource, serviceSource, apiSource, loginSource, helperSource, generatedSdk, zh, en] = await Promise.all([
  readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8').then(JSON.parse),
  readFile(new URL('../../crates/af-http/src/organization_sso_login.rs', import.meta.url), 'utf8'),
  readFile(new URL('../../crates/af-admin/src/organization_sso_login.rs', import.meta.url), 'utf8'),
  readFile(new URL('../src/features/auth/organization-sso-discovery-api.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/features/auth/login-page.tsx', import.meta.url), 'utf8'),
  readFile(new URL('../src/features/organization-sso/organization-sso-authn-request.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/lib/api/generated/sdk.gen.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8').then(JSON.parse),
  readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8').then(JSON.parse),
])

test('公开企业 SSO 启动契约只接受服务端解析标识', () => {
  const operation = openapi.paths['/api/auth/organization-sso/login/start'].post
  assert.equal(operation.operationId, 'startPublicOrganizationSsoLogin')
  assert.equal(
    operation.requestBody.content['application/json'].schema.$ref,
    '#/components/schemas/OrganizationSsoPublicLoginStartRequest',
  )
  assert.deepEqual(
    Object.keys(openapi.components.schemas.OrganizationSsoPublicLoginStartRequest.properties),
    ['identifier'],
  )
  assert.deepEqual(Object.keys(operation.responses).sort(), ['200', '400', '422', '503'])
  assert.match(httpSource, /ORGANIZATION_SSO_PUBLIC_LOGIN_START_PATH/)
  assert.match(serviceSource, /fn begin_public/)
  assert.match(serviceSource, /public_login_target/)
  assert.match(generatedSdk, /export const startPublicOrganizationSsoLogin =/)
})

test('公开启动响应只包含闭合 OIDC 或 SAML 浏览器动作', () => {
  const response = openapi.components.schemas.OrganizationSsoPublicLoginStartResponse
  assert.equal(response.discriminator.propertyName, 'protocol')
  assert.equal(response.oneOf.length, 2)
  const oidc = response.oneOf.find((schema) => schema.properties.protocol.enum[0] === 'oidc')
  const saml = response.oneOf.find((schema) => schema.properties.protocol.enum[0] === 'saml')
  assert.deepEqual(Object.keys(oidc.properties).sort(), ['authorization_url', 'expires_at', 'protocol'])
  assert.deepEqual(
    Object.keys(saml.properties).sort(),
    ['action', 'expires_at', 'protocol', 'relay_state', 'saml_request'],
  )
  for (const sensitive of ['organization_id', 'provider_key', 'provider_version', 'state_digest', 'request_digest']) {
    assert.doesNotMatch(JSON.stringify(response), new RegExp(sensitive))
  }
})

test('登录页立即执行启动动作且不持久化敏感材料', () => {
  assert.match(apiSource, /startPublicOrganizationSsoLogin as startPublicOrganizationSsoLoginRequest/)
  assert.match(apiSource, /isPublicLoginStart/)
  assert.match(loginSource, /window\.location\.assign\(response\.authorization_url\)/)
  assert.match(loginSource, /submitOrganizationSsoSamlPostAction/)
  assert.match(loginSource, /organizationSsoStarting/)
  assert.match(loginSource, /organizationIdentifierRef\.current\.trim\(\) === identifier/)
  assert.match(helperSource, /form\.submit\(\)/)
  assert.match(helperSource, /window\.setTimeout\(\(\) => form\.remove\(\), 0\)/)
  assert.doesNotMatch(`${apiSource}\n${loginSource}\n${helperSource}`, /localStorage|sessionStorage/)
  assert.equal(typeof zh.auth.login.organizationSso.continue, 'string')
  assert.equal(typeof zh.auth.login.organizationSso.starting, 'string')
  assert.equal(typeof en.auth.login.organizationSso.continue, 'string')
  assert.equal(typeof en.auth.login.organizationSso.starting, 'string')
})
