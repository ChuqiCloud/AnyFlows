import assert from 'node:assert/strict'
import test from 'node:test'
import { parseOAuthCallbackPayload } from '../src/features/auth/oauth-callback-payload.ts'

const ticket = 'A'.repeat(43)

test('callback accepts built-in tickets and registered method identifiers from either route', () => {
  assert.deepEqual(parseOAuthCallbackPayload('?ticket=' + ticket, ''), { kind: 'ticket', ticket })
  assert.deepEqual(parseOAuthCallbackPayload('', '#/oauth/callback?method=sample:sign-in&ticket=' + ticket), {
    kind: 'ticket', ticket, method: 'sample:sign-in',
  })
  assert.deepEqual(parseOAuthCallbackPayload('?method=sample:sign-in&ticket=' + ticket, ''), {
    kind: 'ticket', ticket, method: 'sample:sign-in',
  })
})

test('callback rejects ambiguous and malformed credentials without choosing a provider', () => {
  for (const query of [
    'ticket=' + ticket + '&ticket=' + ticket,
    'ticket=' + ticket + '&error=cancelled',
    'ticket=' + ticket + '&method=sample:sign-in&method=sample:other',
    'ticket=' + ticket + '&method=https://example.com',
    'ticket=short',
  ]) assert.deepEqual(parseOAuthCallbackPayload('?' + query, ''), { kind: 'failed' })
  assert.deepEqual(parseOAuthCallbackPayload('?ticket=' + ticket, '#/oauth/callback?ticket=' + ticket), { kind: 'failed' })
  assert.deepEqual(parseOAuthCallbackPayload('', '#/oauth/callback?error=cancelled'), { kind: 'cancelled' })
})
