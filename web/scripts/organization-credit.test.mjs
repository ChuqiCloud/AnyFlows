import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

test('企业授信闭环提供分页事实接口和前端文案', async () => {
  const [openapi, zh, en, page, api] = await Promise.all([
    readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8').then(JSON.parse),
    readFile(new URL('../src/features/organization-credit/organization-credit-page.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-credit/organization-credit-api.ts', import.meta.url), 'utf8'),
  ])
  for (const path of [
    '/api/organizations/{organization_id}/credit-terms',
    '/api/organizations/{organization_id}/credit-invoices',
    '/api/organizations/{organization_id}/credit-repayments',
    '/api/organizations/{organization_id}/credit-allocations',
  ]) {
    assert.equal(openapi.paths[path].get.parameters.some((parameter) => parameter.name === 'before'), true)
    assert.equal(openapi.paths[path].get.parameters.some((parameter) => parameter.name === 'limit'), true)
  }
  assert.ok(openapi.components.schemas.CreditInvoiceResponse)
  assert.ok(openapi.components.schemas.CreditRepaymentResponse)
  assert.ok(openapi.components.schemas.CreditAllocationResponse)
  assert.equal(openapi.components.schemas.CreditRepaymentResponse.properties.source_key, undefined)
  assert.equal(openapi.components.schemas.CreditInvoiceResponse.properties.source_key, undefined)
  assert.equal(zh.organizationCredit.title, '企业授信')
  assert.equal(en.organizationCredit.title, 'Credit & terms')
  assert.match(page, /organization\.credit\.read/)
  assert.match(page, /organization\.credit\.manage/)
  assert.match(api, /listOrganizationCreditInvoices/)
  assert.match(api, /listOrganizationCreditAllocations/)
})
