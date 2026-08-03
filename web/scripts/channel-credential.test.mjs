import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import {
  createApiKeyRequest,
  createOAuthCredentialRequest,
  isValidCredentialSecret,
} from '../src/features/channels/channel-credential-model.ts'
import {
  asAdminOAuthProvider,
  isValidOAuthCallbackUrl,
  oauthAuthorizationRemainingSeconds,
  oauthCredentialConnectionUpdated,
} from '../src/features/channels/channel-oauth-model.ts'

const oauthCredential = {
  id: 9,
  channel_id: 2,
  kind: 'oauth',
  status: 'enabled',
  multi_key_mode: null,
  priority: 0,
  weight: 10,
  concurrency: null,
  load_factor_micros: null,
  rate_multiplier_micros: null,
  schedulable: true,
  rate_limited_at: null,
  rate_limit_reset_at: null,
  overload_until: null,
  temp_unschedulable_until: null,
  session_window_start: null,
  session_window_end: null,
  parent_id: null,
  quota_dimension: 'global',
  proxy_id: null,
  oauth_provider: 'codex',
  oauth_account_key: null,
  oauth_project_id: null,
  oauth_revision: 0,
  last_used_at: null,
  created_at: 10,
  updated_at: 10,
}

test('凭据类型化请求不会把 OAuth provider 写入 API Key', () => {
  const schedule = { priority: -2, weight: 7 }
  const apiKey = createApiKeyRequest('sk-test', schedule)
  const oauth = createOAuthCredentialRequest('access-test', 'codex', schedule)

  assert.equal(apiKey.kind, 'api_key')
  assert.equal(apiKey.oauth_provider, null)
  assert.equal(oauth.kind, 'oauth')
  assert.equal(oauth.oauth_provider, 'codex')
  assert.equal(oauth.priority, -2)
  assert.equal(oauth.weight, 7)
})

test('API Key 与 OAuth access token 共享服务端 ASCII 明文边界', () => {
  assert.equal(isValidCredentialSecret('sk-valid_123'), true)
  assert.equal(isValidCredentialSecret(' token'), false)
  assert.equal(isValidCredentialSecret('token\n'), false)
  assert.equal(isValidCredentialSecret('令牌'), false)
  assert.equal(isValidCredentialSecret('a'.repeat(16 * 1024 + 1)), false)
})

test('手动回调必须匹配固定 redirect URI 且携带查询参数', () => {
  const redirect = 'http://localhost:1455/auth/callback'
  assert.equal(isValidOAuthCallbackUrl(`${redirect}?code=abc&state=xyz`, redirect), true)
  assert.equal(isValidOAuthCallbackUrl(`${redirect}?error=access_denied&state=xyz`, redirect), true)
  assert.equal(isValidOAuthCallbackUrl(redirect, redirect), false)
  assert.equal(isValidOAuthCallbackUrl(`${redirect}?foo=bar`, redirect), false)
  assert.equal(isValidOAuthCallbackUrl(`${redirect}?code=abc&state=one&state=two`, redirect), false)
  assert.equal(isValidOAuthCallbackUrl(`${redirect}?code=abc&error=denied&state=xyz`, redirect), false)
  assert.equal(isValidOAuthCallbackUrl('http://localhost:1455/wrong?code=abc', redirect), false)
  assert.equal(isValidOAuthCallbackUrl('http://127.0.0.1:1455/auth/callback?code=abc', redirect), false)
  assert.equal(isValidOAuthCallbackUrl(`${redirect}?code=abc#fragment`, redirect), false)
})

test('自动完成只接受本次 provider 且 OAuth 版本推进的凭据', () => {
  const baseline = { revision: 0 }
  assert.equal(oauthCredentialConnectionUpdated(oauthCredential, 'codex', baseline), false)
  assert.equal(oauthCredentialConnectionUpdated({ ...oauthCredential, oauth_revision: 1 }, 'codex', baseline), true)
  assert.equal(oauthCredentialConnectionUpdated({ ...oauthCredential, oauth_revision: 1 }, 'gemini', baseline), false)
  assert.equal(oauthCredentialConnectionUpdated({ ...oauthCredential, oauth_provider: null, oauth_revision: 1 }, 'codex', baseline), false)
})

test('provider 收敛与授权剩余时间保持闭合', () => {
  assert.equal(asAdminOAuthProvider('gemini'), 'gemini')
  assert.equal(asAdminOAuthProvider('claude_code'), undefined)
  assert.equal(oauthAuthorizationRemainingSeconds(2_001, 1_000), 2)
  assert.equal(oauthAuthorizationRemainingSeconds(999, 1_000), 0)
})

test('OAuth provider 与固定错误终态都有中英文文案', () => {
  const errorCodes = [
    'invalid_request',
    'forbidden',
    'credential_not_found',
    'oauth_provider_not_configured',
    'oauth_credential_provider_mismatch',
    'oauth_authorization_capacity_exceeded',
    'oauth_authorization_not_found',
    'oauth_authorization_expired',
    'oauth_authorization_denied',
    'oauth_upstream_timeout',
    'oauth_upstream_rejected',
    'oauth_upstream_invalid_response',
    'oauth_unavailable',
    'internal_error',
    'unknown',
  ]
  for (const locale of ['zh', 'en']) {
    const messages = JSON.parse(readFileSync(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'))
    for (const provider of ['codex', 'gemini', 'antigravity']) {
      assert.equal(typeof messages.channels.credentials.oauth.providers[provider], 'string')
    }
    for (const code of errorCodes) {
      assert.equal(typeof messages.channels.credentials.oauth.errors[code], 'string')
    }
  }
})
