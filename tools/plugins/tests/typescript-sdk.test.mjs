import assert from 'node:assert/strict'
import test from 'node:test'

import { AnyFlowsPlugin, PluginError } from '../sdk/typescript/anyflows-plugin.ts'

const manifest = { pluginId: 'org.example.test', pluginVersion: '1.0.0' }

function frame(deadlineUnixMs, operation = { operation: 'invoke_capability', connectionId: 'test', publicConfig: {}, capabilityId: 'example.read', capabilityVersion: 1 }) {
  return {
    protocolVersion: 1,
    pluginId: manifest.pluginId,
    pluginVersion: manifest.pluginVersion,
    artifactSha256: 'a'.repeat(64),
    deploymentGeneration: 1,
    requestId: 'test-request',
    deadlineUnixMs,
    operation,
  }
}

test('capability handlers receive cancellation context', async () => {
  let receivedSignal
  const plugin = new AnyFlowsPlugin(manifest, (_operation, context) => {
    receivedSignal = context.signal
    return { ok: true }
  })
  const response = await plugin.respond(frame(Date.now() + 5_000))

  assert.equal(response.result.kind, 'capability')
  assert.deepEqual(response.result.value.output, { ok: true })
  assert.ok(receivedSignal instanceof AbortSignal)
})

test('handler failures return a bounded connector error', async () => {
  const plugin = new AnyFlowsPlugin(manifest, () => {
    throw new PluginError('forbidden', 'not_applied')
  })
  const response = await plugin.respond(frame(Date.now() + 5_000))

  assert.deepEqual(response.result, {
    kind: 'error',
    value: { code: 'forbidden', outcome: 'not_applied' },
  })
})

test('expired handlers are not started', async () => {
  let called = false
  const plugin = new AnyFlowsPlugin(manifest, () => {
    called = true
    return { ok: true }
  })
  const response = await plugin.respond(frame(Date.now() - 1))

  assert.equal(called, false)
  assert.deepEqual(response.result.value, {
    code: 'temporarily_unavailable',
    outcome: 'not_applied',
  })
})

test('timed out handlers receive an abort signal and report unknown outcome', async () => {
  let aborted = false
  const plugin = new AnyFlowsPlugin(manifest, (_operation, context) => new Promise((resolve) => {
    context.signal.addEventListener('abort', () => {
      aborted = true
      resolve({ late: true })
    }, { once: true })
  }))
  const response = await plugin.respond(frame(Date.now() + 100))

  assert.equal(aborted, true)
  assert.deepEqual(response.result.value, {
    code: 'outcome_unknown',
    outcome: 'unknown',
  })
})
