import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildSubscriptionPlanSchema,
  toSubscriptionPlanRequest,
} from '../src/features/subscriptions/subscription-form-model.ts'

const openapi = JSON.parse(readFileSync(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
const messages = { name: 'name', quotaAmount: 'quota' }

test('订阅路由区分本人入口与管理员入口', () => {
  assert.deepEqual(routeFromHash('#/console/subscriptions'), { view: 'subscriptions' })
  assert.deepEqual(routeFromHash('#/console/subscription-management'), {
    view: 'subscription-management',
  })
})

test('订阅计划表单只生成名称、额度与闭合周期', () => {
  const values = { name: '专业版', quotaAmount: '200000', cycle: 'monthly' }
  assert.equal(buildSubscriptionPlanSchema(messages).safeParse(values).success, true)
  assert.deepEqual(toSubscriptionPlanRequest(values), {
    name: '专业版',
    quota_amount: 200000,
    cycle: 'monthly',
  })
  assert.equal(buildSubscriptionPlanSchema(messages).safeParse({
    ...values,
    quotaAmount: '9007199254740992',
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
  assert.deepEqual(
    Object.keys(openapi.components.schemas.AdminSubscriptionPlanCreateRequest.properties).sort(),
    ['cycle', 'name', 'quota_amount'],
  )
  assert.deepEqual(
    Object.keys(openapi.components.schemas.AdminUserSubscriptionBindRequest.properties),
    ['plan_id'],
  )
})
