import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'
import {
  createCredentialRequest,
  credentialFormValues,
  updateCredentialRequest,
} from '../src/features/credentials/credential-form-request.ts'
import {
  formatMicros,
  parseMicros,
  validOAuthToken,
  validPrivateKeyPem,
  validServiceAccountEmail,
  validSimpleSecret,
} from '../src/features/credentials/credential-form-scalar.ts'
import {
  asAdminOAuthProvider,
  isValidOAuthCallbackUrl,
  oauthAuthorizationRemainingSeconds,
  oauthCredentialConnectionUpdated,
} from '../src/features/credentials/credential-oauth-model.ts'

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

test('四类凭据创建请求只发送对应的结构化密文', () => {
  const apiKey = createCredentialRequest({
    ...credentialFormValues(undefined, 'api_key'),
    apiKey: 'sk-test',
  })
  const oauth = createCredentialRequest({
    ...credentialFormValues(undefined, 'oauth'),
    accessToken: 'access-test',
    oauthProvider: 'codex',
  })
  const bedrock = createCredentialRequest({
    ...credentialFormValues(undefined, 'bedrock'),
    accessKeyId: 'AKIA-TEST',
    secretAccessKey: 'secret-test',
    sessionToken: 'session-test',
  })
  const serviceAccount = createCredentialRequest({
    ...credentialFormValues(undefined, 'service_account'),
    clientEmail: 'robot@project.iam.gserviceaccount.com',
    privateKeyId: 'key-id',
    privateKey: '-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n',
  })

  assert.deepEqual(apiKey.secret, { kind: 'api_key', api_key: 'sk-test' })
  assert.equal(apiKey.oauth_provider, null)
  assert.deepEqual(oauth.secret, { kind: 'oauth', access_token: 'access-test' })
  assert.equal(oauth.oauth_provider, 'codex')
  assert.deepEqual(bedrock.secret, {
    kind: 'bedrock',
    access_key_id: 'AKIA-TEST',
    secret_access_key: 'secret-test',
    session_token: 'session-test',
  })
  assert.deepEqual(serviceAccount.secret, {
    kind: 'service_account',
    client_email: 'robot@project.iam.gserviceaccount.com',
    private_key_id: 'key-id',
    private_key: '-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n',
  })
})

test('编辑凭据默认保留密文且完整保留已有代理绑定', () => {
  const credential = { ...oauthCredential, proxy_id: 7, rate_multiplier_micros: 1_250_000 }
  const values = { ...credentialFormValues(credential), weight: '25', rotateSecret: false }
  const request = updateCredentialRequest(values, credential)

  assert.equal(request.secret, null)
  assert.equal(request.proxy_id, 7)
  assert.equal(request.weight, 25)
  assert.equal(request.rate_multiplier_micros, 1_250_000)
})

test('倍率在十进制字符串与百万分整数之间精确换算', () => {
  assert.equal(parseMicros('0'), 0)
  assert.equal(parseMicros('1.234567'), 1_234_567)
  assert.equal(parseMicros('9007199254.740991'), Number.MAX_SAFE_INTEGER)
  assert.equal(parseMicros('9007199254.740992'), undefined)
  assert.equal(parseMicros('1.0000000'), undefined)
  assert.equal(parseMicros('.5'), undefined)
  assert.equal(parseMicros('-1'), undefined)
  assert.equal(formatMicros(1_250_000), '1.25')
  assert.equal(formatMicros(Number.MAX_SAFE_INTEGER), '9007199254.740991')
})

test('各类密文使用与服务端一致的格式边界', () => {
  assert.equal(validSimpleSecret('sk-valid_123'), true)
  assert.equal(validSimpleSecret(' token'), false)
  assert.equal(validOAuthToken('token-value'), true)
  assert.equal(validOAuthToken('token value'), false)
  assert.equal(validServiceAccountEmail('robot@project.iam.gserviceaccount.com'), true)
  assert.equal(validServiceAccountEmail('robot@example.com'), false)
  assert.equal(validPrivateKeyPem('-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n'), true)
  assert.equal(validPrivateKeyPem('-----BEGIN PRIVATE KEY-----\n\tabc\n-----END PRIVATE KEY-----\n'), false)
  assert.equal(validPrivateKeyPem('not-a-private-key'), false)
})

test('凭据路由只接受单个安全正整数渠道参数', () => {
  assert.deepEqual(routeFromHash('#/console/credentials?channel=17'), { view: 'credentials', channelId: 17 })
  assert.deepEqual(routeFromHash('#/console/credentials'), { view: 'credentials', channelId: undefined })
  for (const hash of [
    '#/console/credentials?channel=0',
    '#/console/credentials?channel=-1',
    '#/console/credentials?channel=1.5',
    '#/console/credentials?channel=01',
    '#/console/credentials?channel=1&channel=2',
    '#/console/credentials?channel=9007199254740992',
  ]) assert.deepEqual(routeFromHash(hash), { view: 'credentials', channelId: undefined })
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
  assert.equal(asAdminOAuthProvider('claude_code'), 'claude_code')
  assert.equal(asAdminOAuthProvider('unknown'), undefined)
  assert.equal(oauthAuthorizationRemainingSeconds(2_001, 1_000), 2)
  assert.equal(oauthAuthorizationRemainingSeconds(999, 1_000), 0)
})

test('独立账号池与 OAuth 固定终态都有中英文文案', () => {
  const errorCodes = [
    'invalid_request', 'forbidden', 'credential_not_found',
    'oauth_provider_not_configured', 'oauth_credential_provider_mismatch',
    'oauth_authorization_capacity_exceeded', 'oauth_authorization_not_found',
    'oauth_authorization_expired', 'oauth_authorization_denied',
    'oauth_upstream_timeout', 'oauth_upstream_rejected',
    'oauth_upstream_invalid_response', 'oauth_unavailable', 'internal_error', 'unknown',
  ]
  for (const locale of ['zh', 'en']) {
    const messages = JSON.parse(readFileSync(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'))
    assert.equal(typeof messages.nav.credentials, 'string')
    assert.equal(typeof messages.credentials.title, 'string')
    for (const kind of ['api_key', 'oauth', 'bedrock', 'service_account']) {
      assert.equal(typeof messages.credentials.kind[kind], 'string')
    }
    for (const provider of ['claude_code', 'codex', 'gemini', 'antigravity']) {
      assert.equal(typeof messages.credentials.oauth.providers[provider], 'string')
    }
    for (const code of errorCodes) {
      assert.equal(typeof messages.credentials.oauth.errors[code], 'string')
    }
  }
})
