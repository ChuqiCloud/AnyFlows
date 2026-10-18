import assert from 'node:assert/strict'
import test from 'node:test'
import { credentialFormValues, createCredentialRequest, updateCredentialRequest } from '../src/features/credentials/credential-form-request.ts'
import { credentialKindsForChannel, credentialRuntimeState } from '../src/features/credentials/credential-model.ts'

test('公开凭据不携带 Key，保留独立调度和健康状态', () => {
  const values = { ...credentialFormValues(undefined, 'public'), apiKey: 'must-not-be-sent', concurrency: '3' }
  const created = createCredentialRequest(values)
  assert.equal(created.kind, 'public')
  assert.deepEqual(created.secret, { kind: 'public' })
  assert.equal(created.oauth_provider, null)
  assert.equal(created.concurrency, 3)
  const existing = { kind: 'public', status: 'enabled', schedulable: true, quota_dimension: 'global', last_used_at: 10, rate_limit_reset_at: 100 }
  const updated = updateCredentialRequest(values, existing)
  assert.equal(updated.secret, null)
  assert.equal(updated.kind, 'public')
  assert.equal(credentialRuntimeState(existing, 50), 'cooling')
  assert.equal(credentialRuntimeState(existing, 101), 'available')
  assert.deepEqual(credentialKindsForChannel('opencode'), ['api_key', 'public'])
  assert.ok(!credentialKindsForChannel('openai').includes('public'))
})
