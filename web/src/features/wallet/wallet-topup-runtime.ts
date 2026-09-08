import type { Appearance } from '@stripe/stripe-js'
import { useEffect, useState } from 'react'

import { ApiError } from '@/lib/api'
import { parseStoredTopupAttempt, type TopupAttempt } from './topup-form-model'

const topupAttemptStorageKey = 'anyflows.wallet.topup-attempt'
const stripeReturnParameters = [
  'payment_intent',
  'payment_intent_client_secret',
  'redirect_status',
] as const
const epayReturnParameters = [
  'pid',
  'trade_no',
  'out_trade_no',
  'type',
  'name',
  'money',
  'trade_status',
  'sign',
  'sign_type',
] as const

export const publicTopupAmountBounds = { min: 50, max: 99_999_999 } as const
export type TopupConfirmationPhase = 'succeeded' | 'processing' | 'failed'

/** 读取并校验当前浏览器会话中尚未完成的充值尝试。 */
export function readStoredTopupAttempt(scope = 'personal') {
  try {
    return parseStoredTopupAttempt(
      sessionStorage.getItem(topupStorageKey(scope)),
      publicTopupAmountBounds,
    )
  } catch {
    return undefined
  }
}

/** 仅持久化金额、支付方式与幂等键，绝不保存支付会话密钥。 */
export function storeTopupAttempt(attempt: TopupAttempt, scope = 'personal') {
  try {
    sessionStorage.setItem(topupStorageKey(scope), JSON.stringify(attempt))
  } catch {
    // 会话存储不可用时仍可完成当前页面内的幂等重试。
  }
}

/** 清理已到账或由用户明确放弃的浏览器侧充值尝试。 */
export function clearStoredTopupAttempt(scope = 'personal') {
  try {
    sessionStorage.removeItem(topupStorageKey(scope))
  } catch {
    // 存储清理失败不影响服务端订单终态。
  }
}

/** 消费支付跳转结果并移除签名材料；易支付结果只触发本地订单状态恢复。 */
export function usePaymentRedirectStatus() {
  const [status] = useState<TopupConfirmationPhase | undefined>(() => {
    const parameters = paymentReturnParameters(new URL(window.location.href))
    const value = parameters.get('redirect_status')
    if (value === 'succeeded' || value === 'processing' || value === 'failed') return value
    if (epayReturnParameters.some((parameter) => parameters.has(parameter))) return 'processing'
    return undefined
  })
  useEffect(() => {
    const url = new URL(window.location.href)
    let changed = false
    for (const parameter of stripeReturnParameters) {
      if (!url.searchParams.has(parameter)) continue
      url.searchParams.delete(parameter)
      changed = true
    }
    for (const parameter of epayReturnParameters) {
      if (!url.searchParams.has(parameter)) continue
      url.searchParams.delete(parameter)
      changed = true
    }
    const hashQueryStart = url.hash.indexOf('?')
    if (hashQueryStart >= 0) {
      const hashPath = url.hash.slice(0, hashQueryStart)
      const hashParameters = new URLSearchParams(url.hash.slice(hashQueryStart + 1))
      for (const parameter of [...stripeReturnParameters, ...epayReturnParameters]) {
        if (!hashParameters.has(parameter)) continue
        hashParameters.delete(parameter)
        changed = true
      }
      const remaining = hashParameters.toString()
      url.hash = remaining ? `${hashPath}?${remaining}` : hashPath
    }
    if (changed) window.history.replaceState(window.history.state, '', url)
  }, [])
  return status
}

/** 返回支付完成后回到本人钱包的固定地址。 */
export function topupReturnUrl(hash = '#/console/wallet') {
  const url = new URL(window.location.href)
  for (const parameter of stripeReturnParameters) url.searchParams.delete(parameter)
  for (const parameter of epayReturnParameters) url.searchParams.delete(parameter)
  url.hash = hash
  return url.toString()
}

function topupStorageKey(scope: string) {
  return scope === 'personal' ? topupAttemptStorageKey : `${topupAttemptStorageKey}.${scope}`
}

/** 合并普通查询串和 hash 查询串，兼容聚合支付网关的返回地址拼接方式。 */
function paymentReturnParameters(url: URL) {
  const parameters = new URLSearchParams(url.search)
  const hashQueryStart = url.hash.indexOf('?')
  if (hashQueryStart < 0) return parameters
  for (const [key, value] of new URLSearchParams(url.hash.slice(hashQueryStart + 1))) {
    parameters.append(key, value)
  }
  return parameters
}

/** 跟随全局主题切换 Stripe iframe 外观，不复制站内主题状态。 */
export function useRootTheme() {
  const current = () => document.documentElement.classList.contains('light') ? 'light' : 'dark'
  const [theme, setTheme] = useState<'dark' | 'light'>(current)
  useEffect(() => {
    const observer = new MutationObserver(() => setTheme(current()))
    observer.observe(document.documentElement, { attributeFilter: ['class'], attributes: true })
    return () => observer.disconnect()
  }, [])
  return theme
}

/** 将站内语义 token 映射为 Stripe Payment Element 外观变量。 */
export function stripeTopupAppearance(theme: 'dark' | 'light'): Appearance {
  const styles = getComputedStyle(document.documentElement)
  const token = (name: string) => styles.getPropertyValue(name).trim()
  return {
    theme: theme === 'light' ? 'stripe' : 'night',
    variables: {
      borderRadius: '12px',
      colorBackground: token('--surface-sunken'),
      colorDanger: token('--destructive'),
      colorPrimary: token('--primary'),
      colorText: token('--foreground'),
      colorTextSecondary: token('--muted-foreground'),
      fontFamily: token('--font-sans'),
    },
  }
}

/** 从统一 API 错误中读取公开稳定错误码。 */
export function apiErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}

/** 将充值 API 错误收敛为前端闭合文案键。 */
export function topupErrorKey(code: string | undefined) {
  if (code === 'topup_unavailable') return 'unavailable'
  if (code === 'topup_provider_rejected') return 'providerRejected'
  if (code === 'topup_order_conflict') return 'conflict'
  if (code === 'topup_order_outcome_unknown') return 'outcomeUnknown'
  if (code === 'invalid_request') return 'invalid'
  return 'unknown'
}
