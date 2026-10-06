import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildSiteSettingsSchema,
  siteSettingsValues,
  toSiteSettingsRequest,
} from '../src/features/site-settings/site-settings-form-model.ts'

const messages = {
  siteName: 'site-name',
  publicBaseUrl: 'base-url',
  logoUrl: 'logo-url',
  tagline: 'tagline',
  description: 'description',
  unitName: 'unit-name',
  unitSymbol: 'unit-symbol',
  quotaUnitsPerDisplayUnit: 'quota-scale',
  fractionDigits: 'fraction-digits',
}

test('站点设置使用独立管理员路由', () => {
  assert.deepEqual(routeFromHash('#/console/system-settings/site'), { view: 'site-settings' })
})

test('站点设置表单接受公开地址与站内 Logo 并归一化空值', () => {
  const values = siteSettingsValues({
    site_name: 'AnyFlows Cloud',
    public_base_url: 'https://example.com/gateway',
    brand: {
      logo_url: '/brand.svg',
      tagline: '统一访问模型',
      description: '面向团队的模型 API 工作台。',
    },
    balance_display: {
      mode: 'quota',
      unit_name: '算力积分',
      unit_symbol: '积分',
      quota_units_per_display_unit: '10000',
      symbol_position: 'suffix',
      fraction_digits: 0,
    },
    version: 2,
  })
  assert.equal(buildSiteSettingsSchema(messages).safeParse(values).success, true)
  assert.equal(buildSiteSettingsSchema(messages).safeParse({
    ...values,
    publicBaseUrl: 'https://user@example.com',
  }).success, false)
  assert.equal(buildSiteSettingsSchema(messages).safeParse({
    ...values,
    logoUrl: 'javascript:alert(1)',
  }).success, false)
  assert.deepEqual(toSiteSettingsRequest({ ...values, publicBaseUrl: '', tagline: '' }, 2), {
    site_name: 'AnyFlows Cloud',
    public_base_url: null,
    brand: {
      logo_url: '/brand.svg',
      tagline: null,
      description: '面向团队的模型 API 工作台。',
    },
    balance_display: {
      mode: 'quota',
      unit_name: '算力积分',
      unit_symbol: '积分',
      quota_units_per_display_unit: '10000',
      symbol_position: 'suffix',
      fraction_digits: 0,
    },
    expected_version: 2,
  })
})

test('旧版后端缺少余额展示字段时回退到原始额度模式', () => {
  const values = siteSettingsValues({
    site_name: 'AnyFlows',
    public_base_url: null,
    brand: {
      logo_url: null,
      tagline: null,
      description: null,
    },
    version: 1,
  })

  assert.deepEqual({
    mode: values.balanceMode,
    unitName: values.unitName,
    unitSymbol: values.unitSymbol,
    quotaUnitsPerDisplayUnit: values.quotaUnitsPerDisplayUnit,
    symbolPosition: values.symbolPosition,
    fractionDigits: values.fractionDigits,
  }, {
    mode: 'quota',
    unitName: '算力积分',
    unitSymbol: '积分',
    quotaUnitsPerDisplayUnit: '10000',
    symbolPosition: 'suffix',
    fractionDigits: 0,
  })
})
