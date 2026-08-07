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

export const publicTopupAmountBounds = { min: 50, max: 99_999_999 } as const
export type TopupConfirmationPhase = 'succeeded' | 'processing' | 'failed'

/** 读取并校验当前浏览器会话中尚未完成的充值尝试。 */
export function readStoredTopupAttempt() {
  try {
    return parseStoredTopupAttempt(
      sessionStorage.getItem(topupAttemptStorageKey),
      publicTopupAmountBounds,
    )
  } catch {
    return undefined
  }
}

/** 仅持久化金额与幂等键，绝不保存 Stripe client secret。 */
export function storeTopupAttempt(attempt: TopupAttempt) {
  try {
    sessionStorage.setItem(topupAttemptStorageKey, JSON.stringify(attempt))
  } catch {
    // 会话存储不可用时仍可完成当前页面内的幂等重试。
  }
}

/** 清理已到账或由用户明确放弃的浏览器侧充值尝试。 */
export function clearStoredTopupAttempt() {
  try {
    sessionStorage.removeItem(topupAttemptStorageKey)
  } catch {
    // 存储清理失败不影响服务端订单终态。
  }
}

/** 消费 Stripe 跳转结果并立即从地址栏移除客户端密钥参数。 */
export function useStripeRedirectStatus() {
  const [status] = useState<TopupConfirmationPhase | undefined>(() => {
    const value = new URLSearchParams(window.location.search).get('redirect_status')
    if (value === 'succeeded' || value === 'processing' || value === 'failed') return value
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
    if (changed) window.history.replaceState(window.history.state, '', url)
  }, [])
  return status
}

/** 返回 Stripe 跳转支付完成后回到本人钱包的地址。 */
export function stripeTopupReturnUrl() {
  const url = new URL(window.location.href)
  for (const parameter of stripeReturnParameters) url.searchParams.delete(parameter)
  url.hash = '#/console/wallet'
  return url.toString()
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
