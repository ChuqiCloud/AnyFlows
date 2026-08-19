import assert from 'node:assert/strict'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'

test('通知偏好深链解析为个人资料通知区', () => {
  assert.deepEqual(routeFromHash('#/console/profile?section=notifications'), {
    view: 'profile',
    profileSection: 'notifications',
  })
})

test('普通个人资料路由不携带区块状态', () => {
  assert.deepEqual(routeFromHash('#/console/profile'), { view: 'profile' })
})

test('重复通知区块参数被拒绝', () => {
  assert.deepEqual(routeFromHash('#/console/profile?section=notifications&section=notifications'), { view: 'profile' })
})

test('未知个人资料区块被拒绝', () => {
  assert.deepEqual(routeFromHash('#/console/profile?section=security'), { view: 'profile' })
})
