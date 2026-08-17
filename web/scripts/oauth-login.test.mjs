import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'

test('OAuth 回调路由不把一次性票据写入应用路由状态', () => {
  assert.deepEqual(routeFromHash(`#/oauth/callback?ticket=${'A'.repeat(43)}`), {
    view: 'oauth-callback',
  })
})

test('OAuth 回调页在交换前清理 hash 且不使用持久化存储', () => {
  const source = readFileSync(
    new URL('../src/features/auth/oauth-callback-page.tsx', import.meta.url),
    'utf8',
  )
  assert.match(source, /history\.replaceState/)
  assert.match(source, /useState\(consumeCallbackPayloadOnce\)/)
  assert.match(source, /consumedCallbackPayload \?\?= consumeCallbackPayload\(\)/)
  assert.match(source, /exchangeOAuthTicket/)
  assert.doesNotMatch(source, /localStorage|sessionStorage/)
})

test('登录页只消费公开 Provider 投影并请求服务端授权地址', () => {
  const source = readFileSync(
    new URL('../src/features/auth/login-page.tsx', import.meta.url),
    'utf8',
  )
  assert.match(source, /authentication\.oauth_providers/)
  assert.match(source, /beginGitHubOAuthLogin/)
  assert.doesNotMatch(source, /github\.com\/login\/oauth\/authorize/)
})
