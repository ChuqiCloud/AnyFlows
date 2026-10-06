import assert from 'node:assert/strict'
import test from 'node:test'
import { alipayAuthorizationUrl, alipayLaunchUrl, alipayRemainingSeconds, isAlipayMobile } from '../src/features/account-verification/alipay-authorization.ts'

const authorization = 'https://openauth.alipay.com/oauth2/publicAppAuthorize.htm?app_id=123&scope=id_verify&redirect_uri=https%3A%2F%2Fexample.com%2Fcallback%3Fa%3D1%26b%3D2&state=0123456789abcdef0123456789abcdef'

test('mobile launch preserves the complete nested callback URL and state', () => {
  const launch = new URL(alipayLaunchUrl(authorization, 'iPhone'))
  assert.equal(launch.protocol, 'alipays:')
  assert.equal(launch.searchParams.get('appId'), '20000067')
  assert.equal(launch.searchParams.get('url'), authorization)
  assert.equal(alipayLaunchUrl(authorization, 'AlipayClient/10.0'), authorization)
  assert.equal(isAlipayMobile('Mozilla Android'), true)
  assert.equal(isAlipayMobile('Mozilla iPhone'), true)
  assert.equal(isAlipayMobile('Mozilla Windows NT'), false)
})

test('untrusted and malformed authorization links cannot be launched', () => {
  for (const url of [null, '', 'javascript:alert(1)', authorization.replace('https:', 'http:'), authorization.replace('openauth.alipay.com', 'openauth.alipay.com.evil.test'), authorization.replace('/oauth2/', '/other/'), authorization.replace('id_verify', 'auth_user'), authorization.replace('0123456789abcdef0123456789abcdef', 'bad'), authorization + '#fragment']) {
    assert.equal(alipayAuthorizationUrl(url), null)
  }
})

test('server time controls expiry even when a phone clock is years ahead', () => {
  const flow = { provider: 'alipay', status: 1, provider_expires_at: 1600, server_time: 1000 }
  const phoneNow = 1_900_000_000_000
  assert.equal(alipayRemainingSeconds(flow, phoneNow, phoneNow), 600)
  assert.equal(alipayRemainingSeconds(flow, phoneNow, phoneNow + 599_000), 1)
  assert.equal(alipayRemainingSeconds(flow, phoneNow, phoneNow + 600_000), 0)
  assert.equal(alipayRemainingSeconds(flow, phoneNow, phoneNow + 900_000), 0)
  assert.equal(alipayRemainingSeconds({ ...flow, status: 4 }, phoneNow, phoneNow), 0)
  assert.equal(alipayRemainingSeconds({ ...flow, provider_status: 'expired' }, phoneNow, phoneNow), 0)
  assert.equal(alipayRemainingSeconds({ ...flow, server_time: undefined }, phoneNow, phoneNow), 0)
  assert.equal(alipayRemainingSeconds({ ...flow, provider_expires_at: null }, phoneNow, phoneNow), 0)
})
