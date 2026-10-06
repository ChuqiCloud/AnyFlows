import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

import { classifyPlaygroundError } from '../src/features/playground/playground-error.ts'

function source(relativePath) {
  return readFileSync(new URL(relativePath, import.meta.url), 'utf8')
}

test('试炼场只使用管理会话和专用网关路径', () => {
  const client = source('../src/features/playground/playground-client.ts')
  const configuration = source('../src/features/playground/use-playground-configuration.ts')
  const comparison = source('../src/features/playground/use-playground-comparison.ts')
  const videoClient = source('../src/features/video-tasks/video-task-client.ts')

  assert.match(client, /getManagementSessionToken/)
  assert.match(client, /\/api\/playground\/v1/)
  assert.match(client, /headers\.delete\('x-api-key'\)/)
  assert.match(client, /headers\.delete\('x-goog-api-key'\)/)
  assert.doesNotMatch(configuration, /apiKey|usePlaygroundApiKey|provision/)
  assert.doesNotMatch(comparison, /settings\.apiKey/)
  assert.match(videoClient, /getManagementSessionToken/)
  assert.match(videoClient, /\/api\/playground\/v1\/videos/)
})

test('试炼场源码不再签发或持久化托管 Key', () => {
  const page = source('../src/features/playground/playground-page.tsx')
  const settings = source('../src/features/playground/playground-settings.tsx')
  const workspace = source('../src/features/video-tasks/use-video-task-workspace.ts')

  for (const contents of [page, settings, workspace]) {
    assert.doesNotMatch(contents, /createPlaygroundToken|sessionStorage|managed-api-key|provisionApiKey/)
  }
})

test('SDK 错误只映射稳定分类，不透出响应正文', () => {
  assert.equal(classifyPlaygroundError({ statusCode: 401 }), 'invalid_key')
  assert.equal(classifyPlaygroundError({
    statusCode: 429,
    responseBody: '{"error":{"code":"insufficient_quota","message":"secret"}}',
  }), 'insufficient_quota')
  assert.equal(classifyPlaygroundError({ statusCode: 429 }), 'rate_limited')
  assert.equal(classifyPlaygroundError({ statusCode: 404 }), 'model_unavailable')
  assert.equal(classifyPlaygroundError({ statusCode: 413 }), 'invalid_request')
  assert.equal(classifyPlaygroundError({ statusCode: 503 }), 'upstream_unavailable')
  assert.equal(classifyPlaygroundError({
    type: 'error',
    code: 'server_error',
    message: 'Upstream request failed.',
  }), 'upstream_unavailable')
  assert.equal(classifyPlaygroundError({ code: 'rate_limit_exceeded' }), 'rate_limited')
  assert.equal(classifyPlaygroundError({ code: 'model_not_found' }), 'model_unavailable')
  assert.equal(classifyPlaygroundError({ statusCode: 500, responseBody: 'private' }), 'unknown')
})
