import assert from 'node:assert/strict'
import { afterEach, test } from 'node:test'

import {
  clearManagementSessionToken,
  getManagementSessionToken,
  invalidateManagementSession,
  setManagementSessionToken,
  subscribeManagementSessionInvalidated,
} from '../src/lib/api/session-token.ts'

const originalStorageDescriptor = Object.getOwnPropertyDescriptor(globalThis, 'sessionStorage')

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
  clearManagementSessionToken()

  if (originalStorageDescriptor) {
    Object.defineProperty(globalThis, 'sessionStorage', originalStorageDescriptor)
  } else {
    delete globalThis.sessionStorage
  }
})

test('管理令牌可在当前标签页存储、恢复并清除', () => {
  const storage = createMemoryStorage()
  installStorage(storage)

  setManagementSessionToken('token-1')
  assert.equal(getManagementSessionToken(), 'token-1')

  clearManagementSessionToken()
  assert.equal(getManagementSessionToken(), undefined)
  assert.equal(storage.length, 0)
})

test('存储读取为空时保留当前页面内存令牌', () => {
  installStorage({
    getItem() {
      return null
    },
    removeItem() {},
    setItem() {},
  })

  setManagementSessionToken('memory-fallback')
  assert.equal(getManagementSessionToken(), 'memory-fallback')
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

  setManagementSessionToken('memory-only')
  assert.equal(getManagementSessionToken(), 'memory-only')

  clearManagementSessionToken()
  assert.equal(getManagementSessionToken(), undefined)
})

test('只有现有会话收到 401 时才广播失效事件', () => {
  installStorage(createMemoryStorage())
  let invalidatedCount = 0
  const unsubscribe = subscribeManagementSessionInvalidated(() => {
    invalidatedCount += 1
  })

  assert.equal(invalidateManagementSession(), false)
  assert.equal(invalidatedCount, 0)

  setManagementSessionToken('expired-token')
  assert.equal(invalidateManagementSession(), true)
  assert.equal(getManagementSessionToken(), undefined)
  assert.equal(invalidatedCount, 1)

  unsubscribe()
})
