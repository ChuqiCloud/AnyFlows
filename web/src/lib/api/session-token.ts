const storageKey = 'anyflows.management.access-token'

type SessionInvalidatedListener = () => void

let memoryToken: string | undefined
let storageAccessBlocked = false
const invalidatedListeners = new Set<SessionInvalidatedListener>()

function getSessionStorage() {
  // 隐私防护可能让 storage getter 或其方法抛异常；失败后固定退化到页面内存，
  // 避免每个 API 请求都再次触发浏览器的 Tracking Prevention 告警。
  if (storageAccessBlocked) return undefined

  try {
    return globalThis.sessionStorage
  } catch {
    storageAccessBlocked = true
    return undefined
  }
}

/** 读取当前管理会话令牌，浏览器存储不可用时退化到页面内存。 */
export function getManagementSessionToken() {
  const storage = getSessionStorage()

  if (storage) {
    try {
      const storedToken = storage.getItem(storageKey)
      // 某些隐私策略会让 storage 读取返回空值但仍允许页面继续运行；
      // 已在本页签签发的令牌应继续使用内存副本，避免后续请求突然变成 401。
      if (storedToken) memoryToken = storedToken
      return memoryToken
    } catch {
      // 隐私模式或浏览器策略可能阻止访问 sessionStorage。
      storageAccessBlocked = true
    }
  }

  return memoryToken
}

/** 保存原始访问令牌，Bearer 前缀由生成客户端统一添加。 */
export function setManagementSessionToken(token: string) {
  const normalizedToken = token.trim()
  if (!normalizedToken) {
    throw new Error('管理会话令牌不能为空')
  }

  memoryToken = normalizedToken

  try {
    getSessionStorage()?.setItem(storageKey, normalizedToken)
  } catch {
    // 内存副本仍可支撑当前页面，会话不会扩散到其他标签页。
    storageAccessBlocked = true
  }
}

/** 清除管理会话，但不广播过期提示，供主动退出与角色拒绝使用。 */
export function clearManagementSessionToken() {
  memoryToken = undefined

  try {
    getSessionStorage()?.removeItem(storageKey)
  } catch {
    // 存储不可写时清理内存副本即可结束当前页面会话。
    storageAccessBlocked = true
  }
}

/** 使现有会话失效；没有令牌时保持静默，避免污染登录失败提示。 */
export function invalidateManagementSession() {
  if (!getManagementSessionToken()) {
    return false
  }

  clearManagementSessionToken()

  for (const listener of invalidatedListeners) {
    try {
      listener()
    } catch {
      // 单个订阅方异常不能改变原始 API 错误的传播语义。
    }
  }

  return true
}

/** 订阅服务端 401 导致的会话失效事件。 */
export function subscribeManagementSessionInvalidated(listener: SessionInvalidatedListener) {
  invalidatedListeners.add(listener)

  return () => {
    invalidatedListeners.delete(listener)
  }
}
