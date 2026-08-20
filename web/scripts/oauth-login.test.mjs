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
  assert.match(source, /beginOAuthLogin/)
  assert.match(source, /provider\.id === 'discord'/)
  assert.match(source, /provider\.id === 'telegram'/)
  assert.doesNotMatch(source, /github\.com\/login\/oauth\/authorize/)
})

test('Telegram OAuth 使用生成客户端并接入管理员设置', () => {
  const apiSource = readFileSync(
    new URL('../src/features/auth/oauth-login-api.ts', import.meta.url),
    'utf8',
  )
  const settingsSource = readFileSync(
    new URL('../src/features/authentication-settings/authentication-settings-page.tsx', import.meta.url),
    'utf8',
  )
  assert.match(apiSource, /telegram: \(\) => startTelegramLogin/)
  assert.match(apiSource, /getAdminTelegramOAuthLoginSettings/)
  assert.match(apiSource, /updateAdminTelegramOAuthLoginSettings/)
  assert.match(settingsSource, /provider="telegram"/)
})

test('OAuth 管理设置按 Provider 分开渲染并保持独立查询键', () => {
  const source = readFileSync(
    new URL('../src/features/authentication-settings/oauth-provider-settings.tsx', import.meta.url),
    'utf8',
  )
  assert.match(source, /useAdminOAuthLoginSettings\(provider\)/)
  assert.match(source, /useUpdateAdminOAuthLoginSettings\(provider\)/)
  assert.match(source, /authenticationSettings\.oauth\.\$\{provider\}/)
})
