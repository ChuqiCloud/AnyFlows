import assert from 'node:assert/strict'
import test from 'node:test'
import { invitationAuthLink, oauthInvitationQuery, readInvitationContext } from '../src/features/auth/invitation-context.ts'

const code = 'af-ABCDEFGHIJKLMNOPQRSTUV'

test('invitation links preserve attribution across login and registration without storage', () => {
  for (const path of ['/login', '/register']) {
    const link = invitationAuthLink(path, code)
    assert.deepEqual(readInvitationContext(new URL(link, 'https://example.test').search), { kind: 'valid', code })
    assert.equal(invitationAuthLink(path), path)
  }
  assert.deepEqual(readInvitationContext('?unrelated=value'), { kind: 'none' })
})

test('ambiguous or malformed invitation URLs are rejected before choosing a referrer', () => {
  for (const search of [
    '?invite=', '?invite=bad', `?invite=${code}&invite=${code}`,
    `?invite=${code}&invite=af-1234567890123456789012`, `?invite=${code}%20`,
  ]) {
    assert.deepEqual(readInvitationContext(search), { kind: 'invalid' }, search)
  }
})

test('OAuth starts carry only an explicitly supplied valid invitation', () => {
  assert.deepEqual(oauthInvitationQuery(code), { invite_code: code })
  assert.equal(oauthInvitationQuery(), undefined)
  for (const value of ['', 'bad', ` ${code}`, `${code}\n`]) assert.throws(() => oauthInvitationQuery(value))
})
