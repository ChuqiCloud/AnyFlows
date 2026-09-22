import type { PaymentMethod, PaymentProvider } from '@/features/payment-settings/payment-settings-types'

export type TopupAmountBounds = Readonly<{
  min: number
  max: number
}>

export type TopupAttempt = Readonly<{
  idempotencyKey: string
  amountMinor: number
  paymentMethod: PaymentMethod
  provider: PaymentProvider
}>

type RandomValues = (buffer: Uint8Array<ArrayBuffer>) => Uint8Array<ArrayBuffer>

const idempotencyKeyPattern = /^[0-9a-f]{32}$/

/** 将结构化美元输入精确转换为整数美分，避免浮点金额进入 API。 */
export function topupAmountMinor(value: string, bounds: TopupAmountBounds) {
  const match = /^(0|[1-9]\d{0,8})(?:\.(\d{1,2}))?$/.exec(value.trim())
  if (!match) return undefined

  const whole = Number(match[1])
  const fraction = Number((match[2] ?? '').padEnd(2, '0'))
  const amountMinor = whole * 100 + fraction
  if (!Number.isSafeInteger(amountMinor) || amountMinor < bounds.min || amountMinor > bounds.max) {
    return undefined
  }
  return amountMinor
}

/** 生成浏览器侧 128 位小写十六进制幂等键。 */
export function createTopupIdempotencyKey(
  randomValues: RandomValues = (buffer) => crypto.getRandomValues(buffer),
) {
  const buffer = new Uint8Array(new ArrayBuffer(16))
  return Array.from(randomValues(buffer), (byte) => byte.toString(16).padStart(2, '0')).join('')
}

/** 为一次明确金额创建可跨失败重试复用的充值尝试。 */
export function createTopupAttempt(
  amountMinor: number,
  provider: PaymentProvider,
  paymentMethod: PaymentMethod,
  randomValues?: RandomValues,
): TopupAttempt {
  return {
    amountMinor,
    idempotencyKey: createTopupIdempotencyKey(randomValues),
    paymentMethod,
    provider,
  }
}

/** 校验从会话存储恢复的充值尝试，拒绝损坏或越界数据。 */
export function parseStoredTopupAttempt(
  value: string | null,
  bounds: TopupAmountBounds,
): TopupAttempt | undefined {
  if (!value) return undefined
  try {
    const parsed = JSON.parse(value) as Partial<TopupAttempt>
    if (
      typeof parsed.idempotencyKey !== 'string'
      || !idempotencyKeyPattern.test(parsed.idempotencyKey)
      || !Number.isSafeInteger(parsed.amountMinor)
      || !validPaymentProvider(parsed.provider)
      || !validPaymentMethod(parsed.paymentMethod)
      || (parsed.amountMinor ?? 0) < bounds.min
      || (parsed.amountMinor ?? 0) > bounds.max
    ) {
      return undefined
    }
    return {
      idempotencyKey: parsed.idempotencyKey,
      amountMinor: parsed.amountMinor as number,
      paymentMethod: parsed.paymentMethod,
      provider: parsed.provider,
    }
  } catch {
    return undefined
  }
}

function validPaymentProvider(value: unknown): value is PaymentProvider {
  return value === 'stripe' || value === 'epay'
}

function validPaymentMethod(value: unknown): value is PaymentMethod {
  return value === 'card' || value === 'alipay' || value === 'wxpay'
}

/** 将整数美分还原为金额输入框需要的两位小数文本。 */
export function topupAmountInput(amountMinor: number) {
  return `${Math.floor(amountMinor / 100)}.${String(amountMinor % 100).padStart(2, '0')}`
}
