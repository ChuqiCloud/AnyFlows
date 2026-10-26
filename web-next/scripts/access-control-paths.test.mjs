import assert from 'node:assert/strict'
import test from 'node:test'

import { requiresAdmin } from '../src/shared/access-control/paths.ts'

test('控制台概览和用户工作台允许普通用户访问', () => {
  for (const path of [
    '/console',
    '/console/',
    '/console/api-keys',
    '/console/wallet',
    '/console/subscriptions',
    '/console/invitations',
    '/console/models',
    '/console/playground',
    '/console/video-tasks',
    '/console/usage-logs',
    '/console/profile',
    '/console/account-verification',
  ]) {
    assert.equal(requiresAdmin(path), false, path)
  }
})

test('管理页面、未知路径和额外空路径段仍要求管理员', () => {
  for (const path of [
    '/console/tokens',
    '/console/users',
    '/console/channels',
    '/console/channel-models',
    '/console/credentials',
    '/console/client-profiles',
    '/console/proxies',
    '/console/routes',
    '/console/runtime-status',
    '/console/debug-traces',
    '/console/refunds',
    '/console/redemption-codes',
    '/console/subscription-management',
    '/console/system-settings/about',
    '/console/system-settings/models',
    '/console/system-settings/model-providers',
    '/console/system-settings/site',
    '/console/system-settings/invitations',
    '/console/account-verification-reviews',
    '/console/not-a-page',
    '/console//users',
    '/console//',
  ]) {
    assert.equal(requiresAdmin(path), true, path)
  }
})

test('控制台外的公共页面不需要管理员', () => {
  for (const path of ['/', '/models', '/api', '/register', '/login', '/share/token']) {
    assert.equal(requiresAdmin(path), false, path)
  }
})
