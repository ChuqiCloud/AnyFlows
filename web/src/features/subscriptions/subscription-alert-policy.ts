import type { UserSubscription } from '@/lib/api/generated/types.gen'

/** 使用与后端一致的整数公式判断订阅是否进入预警区间。 */
export function isSubscriptionAlertDue(
  subscription: UserSubscription,
  thresholdPercent: number,
): boolean {
  const quotaAmount = subscription.quota_amount
  const quotaUsed = subscription.quota_used
  if (
    subscription.status !== 'active'
    || !Number.isSafeInteger(quotaAmount)
    || !Number.isSafeInteger(quotaUsed)
    || !Number.isSafeInteger(thresholdPercent)
    || quotaAmount <= 0
    || quotaUsed < 0
    || quotaUsed > quotaAmount
    || thresholdPercent < 1
    || thresholdPercent > 99
  ) {
    return false
  }

  // 拆分整百与余数，避免接近 JavaScript 安全整数上限时乘以百分比丢失精度。
  const wholeHundreds = Math.floor(quotaAmount / 100) * thresholdPercent
  const remainder = Math.floor((quotaAmount % 100) * thresholdPercent / 100)
  const remainingThreshold = wholeHundreds + remainder
  return quotaAmount - quotaUsed <= remainingThreshold
}

/** 返回仅用于界面展示的四舍五入剩余额度百分比。 */
export function subscriptionRemainingPercent(subscription: UserSubscription): number {
  const quotaAmount = subscription.quota_amount
  const quotaUsed = subscription.quota_used
  if (
    !Number.isSafeInteger(quotaAmount)
    || !Number.isSafeInteger(quotaUsed)
    || quotaAmount <= 0
    || quotaUsed < 0
    || quotaUsed > quotaAmount
  ) {
    return 0
  }
  return Math.round((quotaAmount - quotaUsed) / quotaAmount * 100)
}
