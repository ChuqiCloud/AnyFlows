import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildSubscriptionPlanSchema,
  toSubscriptionPlanRequest,
} from '../src/features/subscriptions/subscription-form-model.ts'

const openapi = JSON.parse(readFileSync(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const messages = {
  name: 'name',
  quotaAmount: 'quota',
  priceCurrency: 'currency',
  priceAmountMinor: 'price',
}

test('订阅路由区分本人入口与管理员入口', () => {
  assert.deepEqual(routeFromHash('#/console/subscriptions'), { view: 'subscriptions' })
  assert.deepEqual(routeFromHash('#/console/subscription-management'), {
    view: 'subscription-management',
  })
})

test('订阅计划表单生成价格快照字段并拒绝非法金额和币种', () => {
  const values = {
    name: '专业版',
    quotaAmount: '200000',
    cycle: 'monthly',
    priceProvider: 'stripe',
    priceCurrency: 'USD',
    priceAmountMinor: '100',
  }
  assert.equal(buildSubscriptionPlanSchema(messages).safeParse(values).success, true)
  assert.deepEqual(toSubscriptionPlanRequest(values), {
    name: '专业版',
    quota_amount: 200000,
    cycle: 'monthly',
    price_provider: 'stripe',
    price_currency: 'USD',
    price_amount_minor: 100,
  })
  assert.equal(buildSubscriptionPlanSchema(messages).safeParse({
    ...values,
    quotaAmount: '9007199254740992',
  }).success, false)
  assert.equal(buildSubscriptionPlanSchema(messages).safeParse({
    ...values,
    priceCurrency: 'usd',
  }).success, false)
  assert.equal(buildSubscriptionPlanSchema(messages).safeParse({
    ...values,
    priceAmountMinor: '0',
  }).success, false)
})

test('订阅 OpenAPI 不接受客户端覆盖所有权与周期窗口', () => {
  assert.equal(
    openapi.paths['/api/admin/subscription-plans'].get.operationId,
    'listAdminSubscriptionPlans',
  )
  assert.equal(
    openapi.paths['/api/admin/users/{user_id}/subscriptions'].post.operationId,
    'bindAdminUserSubscription',
  )
  assert.equal(
    openapi.paths['/api/account/subscriptions'].get.operationId,
    'listCurrentUserSubscriptions',
  )
  assert.equal(
    openapi.paths['/api/account/subscription-catalog'].get.operationId,
    'listCurrentSubscriptionCatalog',
  )
  assert.equal(
    openapi.paths['/api/account/subscription-orders'].post.operationId,
    'createCurrentSubscriptionOrder',
  )
  assert.equal(
    openapi.paths['/api/account/subscription-orders/{order_id}'].get.operationId,
    'getCurrentSubscriptionOrder',
  )
  assert.equal(
    openapi.paths['/api/account/subscription-orders/{order_id}/payment'].post.operationId,
    'submitCurrentSubscriptionOrderPayment',
  )
  assert.deepEqual(
    Object.keys(openapi.components.schemas.SubscriptionOrderPaymentRequest.properties),
    ['payment_method'],
  )
  assert.deepEqual(
    Object.keys(openapi.paths['/api/account/subscription-orders/{order_id}/payment'].post.responses).sort(),
    ['200', '400', '401', '404', '409', '500', '503'],
  )
  assert.equal(
    openapi.components.schemas.SubscriptionOrderPaymentResponse.properties.order.$ref,
    '#/components/schemas/SubscriptionOrder',
  )
  assert.equal(
    openapi.components.schemas.SubscriptionOrder.properties.provider_order_id,
    undefined,
  )
  assert.equal(
    openapi.components.schemas.SubscriptionPaymentResponse.properties.provider_order_id,
    undefined,
  )
  assert.equal(
    openapi.components.schemas.SubscriptionPaymentResponse.properties.signature,
    undefined,
  )
  assert.deepEqual(
    Object.keys(openapi.paths['/api/account/subscription-orders/{order_id}'].get.responses).sort(),
    ['200', '400', '401', '404', '500'],
  )
  assert.equal(
    openapi.components.schemas.SubscriptionOrder.properties.provider_order_id,
    undefined,
  )
  assert.deepEqual(
    Object.keys(openapi.components.schemas.AdminSubscriptionPlanCreateRequest.properties).sort(),
    ['cycle', 'name', 'price_amount_minor', 'price_currency', 'price_provider', 'quota_amount'],
  )
  assert.equal(
    openapi.components.schemas.AdminSubscriptionPlanCreateRequest.properties.price_currency.pattern,
    '^[A-Z]{3}$',
  )
  assert.equal(
    openapi.components.schemas.SubscriptionCatalogPlan.properties.price_currency.pattern,
    '^[A-Z]{3}$',
  )
  assert.equal(
    openapi.components.schemas.SubscriptionCatalogPlan.properties.plan_version.minimum,
    1,
  )
  assert.deepEqual(
    Object.keys(openapi.components.schemas.SubscriptionOrderCreateRequest.properties).sort(),
    ['idempotency_key', 'plan_id', 'plan_version', 'price_amount_minor', 'price_currency', 'price_provider'],
  )
  assert.deepEqual(
    Object.keys(openapi.paths['/api/account/subscription-orders'].post.responses).sort(),
    ['200', '201', '400', '401', '404', '409', '500', '503'],
  )
  assert.deepEqual(
    Object.keys(openapi.components.schemas.AdminUserSubscriptionBindRequest.properties),
    ['plan_id'],
  )
})

test('subscription purchase only shows effective after current-user facts confirm it', () => {
  const catalog = readFileSync(new URL('../src/features/subscriptions/subscription-catalog.tsx', import.meta.url), 'utf8')
  const api = readFileSync(new URL('../src/features/subscriptions/subscription-api.ts', import.meta.url), 'utf8')
  assert.match(catalog, /order\.status === 'paid' && active/)
  assert.match(catalog, /currentUserSubscriptionsQueryKey/)
  assert.match(catalog, /paymentEffective/)
  assert.match(api, /data\.order\.status === 'paid'/)
  assert.match(api, /invalidateQueries\(\{ queryKey: currentUserSubscriptionsQueryKey \}\)/)
})
