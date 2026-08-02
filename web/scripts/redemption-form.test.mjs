import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  buildRedemptionBatchSchema,
  expirationUnixSeconds,
  toRedemptionBatchRequest,
} from '../src/features/redemptions/redemption-form-model.ts'

const messages = {
  name: 'name',
  quotaAmount: 'quota',
  codeCount: 'count',
  expiresAt: 'expiration',
}

test('兑换码管理路由进入独立管理员页面', () => {
  assert.deepEqual(routeFromHash('#/console/redemption-codes'), { view: 'redemption-codes' })
})

test('永久有效批次生成结构化请求', () => {
  const values = {
    name: '八月活动',
    quotaAmount: '100000',
    codeCount: '25',
    expires: false,
    expiresAt: '',
  }
  assert.equal(buildRedemptionBatchSchema(messages).safeParse(values).success, true)
  assert.deepEqual(toRedemptionBatchRequest(values, 1_900_000_000_000), {
    name: '八月活动',
    quota_amount: 100000,
    code_count: 25,
    expires_at: null,
  })
})

test('到期时间必须在当前时间之后且数量保持安全整数', () => {
  const future = '2035-01-02T03:04'
  const futureSeconds = Math.floor(new Date(future).getTime() / 1_000)
  assert.equal(expirationUnixSeconds(future, futureSeconds * 1_000 - 1_000), futureSeconds)
  assert.equal(expirationUnixSeconds(future, futureSeconds * 1_000), undefined)

  const schema = buildRedemptionBatchSchema(messages)
  assert.equal(schema.safeParse({
    name: '过量批次',
    quotaAmount: '9007199254740992',
    codeCount: '1001',
    expires: true,
    expiresAt: '2000-01-01T00:00',
  }).success, false)
})
