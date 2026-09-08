const storageKey = 'anyflows.playground.managed-api-key'
const legacyStorageKey = 'anyflows.playground.api-key'
const apiKeyPattern = /^sk-af-[A-Za-z0-9_-]{43}$/

let memoryApiKey: string | undefined

function getSessionStorage() {
  try {
    return globalThis.sessionStorage
  } catch {
    return undefined
  }
}

/** 校验 AnyFlows API Key 的公开格式，不替代服务端鉴权。 */
export function isPlaygroundApiKey(value: string) {
  const normalized = value.trim()
  return apiKeyPattern.test(normalized)
}

/** 读取当前标签页的平台托管 Key，旧版手动凭据不会迁移到托管会话。 */
export function getPlaygroundApiKey() {
  const storage = getSessionStorage()

  if (storage) {
    try {
      storage.removeItem(legacyStorageKey)
      const stored = storage.getItem(storageKey)?.trim()
      if (stored && isPlaygroundApiKey(stored)) {
        memoryApiKey = stored
        return memoryApiKey
      }
      if (stored) storage.removeItem(storageKey)
    } catch {
      // 浏览器阻止存储访问时继续使用页面内存，避免把密钥扩散到其他持久化介质。
    }
  }

  return memoryApiKey
}

/** 保存平台签发的托管 Key 到当前标签页；存储不可用时仅保留页面内存副本。 */
export function setPlaygroundApiKey(value: string) {
  const normalized = value.trim()
  if (!isPlaygroundApiKey(normalized)) {
    throw new Error('Playground API Key 格式无效')
  }

  memoryApiKey = normalized
  try {
    getSessionStorage()?.setItem(storageKey, normalized)
  } catch {
    // 内存副本仍可支撑当前页面，请求结束后不会写入其他存储。
  }
}

/** 清除当前标签页及页面内存中的托管 Key。 */
export function clearPlaygroundApiKey() {
  memoryApiKey = undefined
  try {
    getSessionStorage()?.removeItem(storageKey)
    getSessionStorage()?.removeItem(legacyStorageKey)
  } catch {
    // 存储不可写时清理内存副本即可。
  }
}
