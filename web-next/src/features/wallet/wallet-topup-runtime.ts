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
const paymentFlowParameters = ['flow'] as const
const topupReturnParameters = ['topup_order_id', 'topup_payment_intent'] as const
const epayReturnParameters = [
  'epay_return',
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

/** 持久化重试与回跳所需的非敏感标识，不保存支付会话密钥。 */
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
    const url = new URL(window.location.href)
    const parameters = paymentReturnParameters(url)
    if (!matchesStoredTopupReturn(url)) return undefined
    const value = parameters.get('redirect_status')
    if (value === 'succeeded' || value === 'processing' || value === 'failed') return value
    if (epayReturnParameters.some((parameter) => parameters.has(parameter))) return 'processing'
    return undefined
  })
  useEffect(() => {
    const url = new URL(window.location.href)
    const flow = url.searchParams.get('flow')
    let changed = false
    if (flow === 'subscription') {
      if (url.searchParams.has('payment_intent_client_secret')) {
        url.searchParams.delete('payment_intent_client_secret')
        changed = true
      }
      if (changed) window.history.replaceState(window.history.state, '', url)
      return
    }
    const parametersToRemove = flow === 'topup'
      ? [...paymentFlowParameters, ...topupReturnParameters, ...stripeReturnParameters, ...epayReturnParameters]
      : [...stripeReturnParameters, ...epayReturnParameters]
    changed = removePaymentReturnParameters(url, parametersToRemove) || changed
    if (changed) window.history.replaceState(window.history.state, '', url)
  }, [])
  return status
}

/** 返回支付完成后回到本人钱包的固定地址。 */
export function topupReturnUrl(
  returnPath = '/console/wallet',
  correlation?: { orderId?: string; paymentIntentId?: string },
) {
  const url = new URL(window.location.href)
  for (const parameter of stripeReturnParameters) url.searchParams.delete(parameter)
  for (const parameter of topupReturnParameters) url.searchParams.delete(parameter)
  for (const parameter of epayReturnParameters) url.searchParams.delete(parameter)
  url.pathname = returnPath
  url.searchParams.set('flow', 'topup')
  if (correlation?.orderId) url.searchParams.set('topup_order_id', correlation.orderId)
  if (correlation?.paymentIntentId) url.searchParams.set('topup_payment_intent', correlation.paymentIntentId)
  return url.toString()
}

function topupStorageKey(scope: string) {
  return scope === 'personal' ? topupAttemptStorageKey : `${topupAttemptStorageKey}.${scope}`
}

/** 支付网关回跳参数统一落在查询串上。 */
function paymentReturnParameters(url: URL) {
  const parameters = new URLSearchParams(url.search)
  const hashQuery = url.hash.indexOf('?')
  if (hashQuery >= 0) {
    for (const [key, value] of new URLSearchParams(url.hash.slice(hashQuery + 1))) {
      parameters.set(key, value)
    }
  }
  return parameters
}

function matchesStoredTopupReturn(url: URL) {
  const parameters = paymentReturnParameters(url)
  const attempt = readStoredTopupAttempt()
  if (!attempt?.orderId || parameters.get('flow') === 'subscription') return false
  if (attempt.provider === 'epay') {
    return parameters.get('out_trade_no') === attempt.orderId
      && epayReturnParameters.some((parameter) => parameter !== 'out_trade_no' && parameters.has(parameter))
  }
  if (parameters.get('flow') !== 'topup' || !attempt.paymentIntentId) return false
  return parameters.get('topup_order_id') === attempt.orderId
    && parameters.get('topup_payment_intent') === attempt.paymentIntentId
    && parameters.get('payment_intent') === attempt.paymentIntentId
}

function removePaymentReturnParameters(url: URL, names: readonly string[]) {
  const wanted = new Set(names)
  let changed = false
  for (const parameter of names) {
    if (!url.searchParams.has(parameter)) continue
    url.searchParams.delete(parameter)
    changed = true
  }
  const hashQuery = url.hash.indexOf('?')
  if (hashQuery < 0) return changed
  const prefix = url.hash.slice(0, hashQuery)
  const hashParameters = new URLSearchParams(url.hash.slice(hashQuery + 1))
  for (const parameter of wanted) {
    if (!hashParameters.has(parameter)) continue
    hashParameters.delete(parameter)
    changed = true
  }
  const query = hashParameters.toString()
  url.hash = query ? `${prefix}?${query}` : prefix
  return changed
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
