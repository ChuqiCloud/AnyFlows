import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { URL } from 'node:url'

import { routeFromHash } from '../src/app-route.ts'

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')

test('合同价路由和导航入口保持独立', () => {
  assert.deepEqual(routeFromHash('#/console/organization-contract-prices'), {
    view: 'organization-contract-prices',
  })
  const navigation = read('../src/components/layout/app-navigation-model.ts')
  assert.match(navigation, /organizationContractPrices/)
})

test('合同价前端覆盖读取、创建、关闭和权限裁剪', () => {
  const page = read('../src/features/organization-contract-prices/organization-contract-price-page.tsx')
  const api = read('../src/features/organization-contract-prices/organization-contract-price-api.ts')

  assert.match(page, /organization\.contract_prices\.read/)
  assert.match(page, /organization\.contract_prices\.manage/)
  assert.match(page, /expected_version/)
  assert.match(page, /effective_until/)
  assert.match(page, /organizationContractPrices\.errors\.intervalConflict/)
  assert.match(api, /listOrganizationContractPrices/)
  assert.match(api, /createOrganizationContractPrice/)
  assert.match(api, /closeOrganizationContractPrice/)
  assert.match(api, /invalidateQueries/)
})

test('合同价中英文文案和 OpenAPI 生成客户端均存在', () => {
  const zh = JSON.parse(read('../src/i18n/locales/zh.json'))
  const en = JSON.parse(read('../src/i18n/locales/en.json'))
  for (const locale of [zh, en]) {
    assert.equal(typeof locale.organizationContractPrices.title, 'string')
    assert.equal(typeof locale.organizationContractPrices.form.cache_creation_5m, 'string')
    assert.equal(typeof locale.organizationContractPrices.errors.conflict, 'string')
  }
  const sdk = read('../src/lib/api/generated/sdk.gen.ts')
  assert.match(sdk, /export const listOrganizationContractPrices/)
  assert.match(sdk, /export const createOrganizationContractPrice/)
  assert.match(sdk, /export const closeOrganizationContractPrice/)
})
