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
  const iconSource = readFileSync(
    new URL('../src/features/auth/oauth-provider-icon.tsx', import.meta.url),
    'utf8',
  )
  assert.match(source, /authentication\.oauth_providers/)
  assert.match(source, /beginOAuthLogin/)
  assert.match(source, /OAuthProviderIcon provider=\{providerId\}/)
  assert.match(source, /provider\.display_name/)
  assert.match(source, /oauthStarting\.custom/)
  assert.match(iconSource, /provider === 'github' \|\| provider === 'google'/)
  assert.match(iconSource, /telegram: Send/)
  assert.match(iconSource, /provider === 'google'/)
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

test('Google OAuth 使用生成客户端、固定设置页和官方入口', () => {
  const apiSource = readFileSync(
    new URL('../src/features/auth/oauth-login-api.ts', import.meta.url),
    'utf8',
  )
  const settingsSource = readFileSync(
    new URL('../src/features/authentication-settings/authentication-settings-page.tsx', import.meta.url),
    'utf8',
  )
  const providerSettingsSource = readFileSync(
    new URL('../src/features/authentication-settings/oauth-provider-settings.tsx', import.meta.url),
    'utf8',
  )
  assert.match(apiSource, /google: \(\) => startGoogleLogin/)
  assert.match(apiSource, /getAdminGoogleOAuthLoginSettings/)
  assert.match(apiSource, /updateAdminGoogleOAuthLoginSettings/)
  assert.match(settingsSource, /provider="google"/)
  assert.match(providerSettingsSource, /https:\/\/accounts\.google\.com/)
})

test('自定义 OAuth2 使用受限 Provider key 和生成客户端启动登录', () => {
  const apiSource = readFileSync(
    new URL('../src/features/auth/oauth-login-api.ts', import.meta.url),
    'utf8',
  )
  const openapi = JSON.parse(readFileSync(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))
  const zh = JSON.parse(readFileSync(new URL('../src/i18n/locales/zh.json', import.meta.url), 'utf8'))
  const en = JSON.parse(readFileSync(new URL('../src/i18n/locales/en.json', import.meta.url), 'utf8'))
  const start = openapi.paths['/api/auth/oauth/custom/{provider_key}/start'].post
  const callback = openapi.paths['/api/auth/oauth/custom/{provider_key}/callback'].get

  assert.equal(start.operationId, 'startCustomOAuth2Login')
  assert.equal(callback.operationId, 'completeCustomOAuth2Login')
  assert.equal(start.parameters[0].schema.pattern, '^custom_[a-z0-9_-]+$')
  assert.equal(callback.parameters[0].schema.pattern, '^custom_[a-z0-9_-]+$')
  assert.equal(openapi.components.schemas.PublicOAuthLoginProvider.properties.display_name.maxLength, 128)
  assert.equal(typeof zh.auth.login.oauth.custom, 'string')
  assert.equal(typeof zh.auth.login.oauthStarting.custom, 'string')
  assert.equal(typeof en.auth.login.oauth.custom, 'string')
  assert.equal(typeof en.auth.login.oauthStarting.custom, 'string')
  assert.match(apiSource, /startCustomOAuth2Login/)
  assert.match(apiSource, /path: \{ provider_key: provider \}/)
  assert.doesNotMatch(apiSource, /url: `\/api\/auth\/oauth\/custom/)
})

test('OAuth 管理设置按 Provider 分开渲染并保持独立查询键', () => {
  const source = readFileSync(
    new URL('../src/features/authentication-settings/oauth-provider-settings.tsx', import.meta.url),
    'utf8',
  )
  assert.match(source, /useAdminOAuthLoginSettings\(provider\)/)
  assert.match(source, /useUpdateAdminOAuthLoginSettings\(provider\)/)
  assert.match(source, /authenticationSettings\.oauth\.\$\{provider\}/)
  assert.match(source, /aria-labelledby=\{`\$\{provider\}-oauth-title`\}/)
  assert.match(source, /disabled=\{mutation\.isPending\}/)
})

test('OAuth 管理设置将后端 404 显示为版本提示', () => {
  const source = readFileSync(
    new URL('../src/features/authentication-settings/oauth-provider-settings.tsx', import.meta.url),
    'utf8',
  )
  assert.match(source, /error instanceof ApiError/)
  assert.match(source, /status === 404/)
  assert.match(source, /oauth\.errors\.unsupported/)
})
