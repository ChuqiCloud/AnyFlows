import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { URL } from 'node:url'

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
test('notification read receipts use server facts and the generated client', () => {
  const api = readFileSync(new URL('../src/features/profile/profile-api.ts', import.meta.url), 'utf8')
  const page = readFileSync(new URL('../src/features/profile/profile-page.tsx', import.meta.url), 'utf8')
  const shell = readFileSync(new URL('../src/components/layout/app-shell.tsx', import.meta.url), 'utf8')

  assert.match(api, /markUserNotificationsRead/)
  assert.match(api, /invalidateQueries\(\{ queryKey: userNotificationsQueryKey \}\)/)
  assert.match(page, /unread_count/)
  assert.match(page, /entry\.read_at/)
  assert.match(page, /markReadMutation\.mutateAsync\(\[id\]\)/)
  assert.match(shell, /pages\[0\]\?\.unread_count/)
})
