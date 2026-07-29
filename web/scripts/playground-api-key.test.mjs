import assert from 'node:assert/strict'
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

  clearPlaygroundApiKey()
  assert.equal(getPlaygroundApiKey(), undefined)
  assert.equal(storage.length, 0)
})

test('非法存储残留会被清除且不会进入页面内存', () => {
  const storage = createMemoryStorage()
  storage.setItem('anyflows.playground.api-key', 'not-a-key')
  installStorage(storage)

  assert.equal(getPlaygroundApiKey(), undefined)
  assert.equal(storage.length, 0)
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
  assert.equal(classifyPlaygroundError({ statusCode: 500, responseBody: 'private' }), 'unknown')
})
