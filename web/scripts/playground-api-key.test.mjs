import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { afterEach, test } from 'node:test'

import {
  clearPlaygroundApiKey,
  getPlaygroundApiKey,
  isPlaygroundApiKey,
  setPlaygroundApiKey,
} from '../src/features/playground/playground-api-key.ts'
import { classifyPlaygroundError } from '../src/features/playground/playground-error.ts'

const originalStorageDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'sessionStorage')
const validApiKey = 'sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8'

function source(relativePath) {
  return readFileSync(new URL(relativePath, import.meta.url), 'utf8')
}

function installStorage(storage) {
  Object.defineProperty(globalThis, 'sessionStorage', {
    configurable: true,
    value: storage,
  })
}

function createMemoryStorage() {
  const values = new Map()

  return {
    clear() {
      values.clear()
    },
    getItem(key) {
      return values.get(key) ?? null
    },
    key(index) {
      return [...values.keys()][index] ?? null
    },
    get length() {
      return values.size
    },
    removeItem(key) {
      values.delete(key)
    },
    setItem(key, value) {
      values.set(key, String(value))
    },
  }
}

afterEach(() => {
  clearPlaygroundApiKey()

  if (originalStorageDescriptor) {
    Object.defineProperty(globalThis, 'sessionStorage', originalStorageDescriptor)
  } else {
    delete globalThis.sessionStorage
  }
})

test('Playground API Key 只接受有界 sk-af- 格式', () => {
  assert.equal(isPlaygroundApiKey(`  ${validApiKey}  `), true)
  assert.equal(isPlaygroundApiKey('sk-af-'), false)
  assert.equal(isPlaygroundApiKey('sk-other-value'), false)
  assert.equal(isPlaygroundApiKey('sk-af-has space'), false)
  assert.equal(isPlaygroundApiKey(`sk-af-${'a'.repeat(42)}`), false)
  assert.equal(isPlaygroundApiKey(`sk-af-${'a'.repeat(44)}`), false)
})

test('API Key 可在当前标签页保存、恢复并清除', () => {
  const storage = createMemoryStorage()
  installStorage(storage)

  setPlaygroundApiKey(`  ${validApiKey}  `)
  assert.equal(getPlaygroundApiKey(), validApiKey)
  assert.equal(storage.getItem('anyflows.playground.managed-api-key'), validApiKey)

  clearPlaygroundApiKey()
  assert.equal(getPlaygroundApiKey(), undefined)
  assert.equal(storage.length, 0)
})

test('旧版手动 Key 与非法托管残留都会被清除', () => {
  const storage = createMemoryStorage()
  storage.setItem('anyflows.playground.api-key', validApiKey)
  storage.setItem('anyflows.playground.managed-api-key', 'not-a-key')
  installStorage(storage)

  assert.equal(getPlaygroundApiKey(), undefined)
  assert.equal(storage.length, 0)
})

test('试炼场只保留托管凭据状态与重试入口', () => {
  const settings = source('../src/features/playground/playground-settings.tsx')
  const configuration = source('../src/features/playground/use-playground-configuration.ts')
  const videoAccess = source('../src/features/video-tasks/video-task-access-panel.tsx')
  const userIssuedDialog = source('../src/features/api-keys/issued-api-key-dialog.tsx')
  const adminIssuedDialog = source('../src/features/tokens/issued-token-dialog.tsx')

  assert.match(settings, /onProvisionApiKey/)
  assert.doesNotMatch(settings, /apiKeyDraft|onSaveApiKey|onClearApiKey|sk-af-/)
  assert.match(videoAccess, /onProvisionApiKey/)
  assert.doesNotMatch(videoAccess, /apiKeyDraft|onSave|onClear|sk-af-/)
  assert.doesNotMatch(configuration, /saveApiKey|clearApiKey|updateApiKeyDraft/)
  assert.doesNotMatch(userIssuedDialog, /setPlaygroundApiKey/)
  assert.doesNotMatch(adminIssuedDialog, /setPlaygroundApiKey/)
})

test('sessionStorage 不可用时退化到页面内存', () => {
  installStorage({
    getItem() {
      throw new Error('blocked')
    },
    removeItem() {
      throw new Error('blocked')
    },
    setItem() {
      throw new Error('blocked')
    },
  })

  setPlaygroundApiKey(validApiKey)
  assert.equal(getPlaygroundApiKey(), validApiKey)

  clearPlaygroundApiKey()
  assert.equal(getPlaygroundApiKey(), undefined)
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
