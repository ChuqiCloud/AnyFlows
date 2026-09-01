import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const [helperSource, panelSource, openapi] = await Promise.all([
  readFile(new URL('../src/features/organization-sso/organization-sso-authn-request.ts', import.meta.url), 'utf8'),
  readFile(new URL('../src/features/organization-sso/organization-sso-panel.tsx', import.meta.url), 'utf8'),
  readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8').then(JSON.parse),
])

test('SAML AuthnRequest 前端只提交受控 HTTPS 表单', () => {
  assert.match(helperSource, /request\.action/)
  assert.match(helperSource, /url\.protocol === 'https:'/)
  assert.match(helperSource, /url\.search === ''/)
  assert.match(helperSource, /url\.hash === ''/)
  assert.match(helperSource, /url\.username === ''/)
  assert.match(helperSource, /url\.password === ''/)
  assert.match(helperSource, /fields\.length !== 2/)
  assert.match(helperSource, /'SAMLRequest'/)
  assert.match(helperSource, /'RelayState'/)
  assert.match(helperSource, /form\.submit\(\)/)
  assert.doesNotMatch(helperSource, /localStorage|sessionStorage/)
})

test('SAML 启动响应 schema 与前端消费字段保持同步', () => {
  const response = openapi.components.schemas.OrganizationSsoLoginStartResponse
  const saml = openapi.components.schemas.OrganizationSsoSamlAuthnRequestResponse
  const field = openapi.components.schemas.OrganizationSsoSamlAuthnRequestFieldResponse
  assert.equal(response.properties.saml_authn_request.$ref, '#/components/schemas/OrganizationSsoSamlAuthnRequestResponse')
  assert.deepEqual(Object.keys(saml.properties).sort(), ['action', 'fields'])
  assert.deepEqual(Object.keys(field.properties).sort(), ['name', 'value'])
  assert.match(panelSource, /submitOrganizationSsoSamlAuthnRequest/)
  assert.match(panelSource, /protocol !== 'saml'/)
})
