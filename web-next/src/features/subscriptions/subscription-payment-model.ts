export type SubscriptionPaymentAttempt = {
  idempotencyKey: string
  orderId?: string
  paymentIntentId?: string
  paymentMethod: 'card' | 'alipay' | 'wxpay'
  planId: string
  provider: string
}

/** 只恢复当前计划及服务商的有效尝试，拒绝损坏的关联标识。 */
export function parseStoredSubscriptionAttempt(value: string | null, plan: { plan_id: string; price_provider: string }): SubscriptionPaymentAttempt | undefined {
  if (!value) return undefined
  try {
    const candidate = JSON.parse(value) as Partial<SubscriptionPaymentAttempt> | null
    if (!candidate || typeof candidate !== 'object' || Array.isArray(candidate)) return undefined
    if (candidate.planId !== plan.plan_id || candidate.provider !== plan.price_provider) return undefined
    if (typeof candidate.idempotencyKey !== 'string' || !/^[0-9a-f]{32}$/.test(candidate.idempotencyKey)) return undefined
    if (candidate.paymentMethod !== 'card' && candidate.paymentMethod !== 'alipay' && candidate.paymentMethod !== 'wxpay') return undefined
    if (!validOptionalId(candidate.orderId) || !validOptionalId(candidate.paymentIntentId)) return undefined
    return {
      idempotencyKey: candidate.idempotencyKey,
      paymentMethod: candidate.paymentMethod,
      planId: candidate.planId,
      provider: candidate.provider,
      ...(candidate.orderId !== undefined ? { orderId: candidate.orderId } : {}),
      ...(candidate.paymentIntentId !== undefined ? { paymentIntentId: candidate.paymentIntentId } : {}),
    }
  } catch {
    return undefined
  }
}

/** 已创建订单必须先进入恢复记录，再发起可能失败的支付准备。 */
export async function submitCreatedSubscriptionPayment<T>(
  attempt: SubscriptionPaymentAttempt,
  orderId: string,
  persistAttempt: (attempt: SubscriptionPaymentAttempt) => void,
  submitPayment: () => Promise<T>,
) {
  persistAttempt({ ...attempt, orderId })
  return submitPayment()
}

function validOptionalId(value: unknown) {
  return value === undefined || (typeof value === 'string' && value.length > 0 && value.trim() === value)
}
