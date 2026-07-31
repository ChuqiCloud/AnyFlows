import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'

test('密码重置路由只接受固定格式令牌并支持忘记密码入口', () => {
  const token = `${'A'.repeat(22)}.${'B'.repeat(43)}`
  assert.deepEqual(routeFromHash('#/forgot-password'), { view: 'forgot-password' })
  assert.deepEqual(routeFromHash(`#/reset-password?token=${token}`), {
    view: 'reset-password',
    resetToken: token,
  })
  assert.deepEqual(routeFromHash('#/reset-password?token=short'), {
    view: 'reset-password',
    resetToken: undefined,
  })
})
