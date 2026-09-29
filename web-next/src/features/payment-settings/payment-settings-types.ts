export type PaymentProvider = 'epay' | 'stripe'

export type PaymentMethod = 'alipay' | 'card' | 'wxpay'

/** 管理端读取的支付配置只暴露密钥是否已配置，不返回任何密钥明文。 */
export type AdminPaymentSettings = {
  epay_alipay_enabled: boolean
  epay_enabled: boolean
  epay_gateway_url: string | null
  epay_merchant_id: string | null
  epay_merchant_key_configured: boolean
  epay_qr_enabled: boolean
  epay_refund_enabled: boolean
  epay_quota_per_cny: number
  epay_wxpay_enabled: boolean
  refund_auto_submit_enabled: boolean
  stripe_enabled: boolean
  stripe_publishable_key: string | null
  stripe_secret_key_configured: boolean
  stripe_signature_tolerance_seconds: number
  stripe_webhook_secret_configured: boolean
  version: number
}

/** 空密钥表示保留服务端已有密文，绝不从读取接口回填。 */
export type AdminPaymentSettingsRequest = {
  clear_epay_merchant_key: boolean
  clear_stripe_secret_key: boolean
  clear_stripe_webhook_secret: boolean
  epay_alipay_enabled: boolean
  epay_enabled: boolean
  epay_gateway_url: string | null
  epay_merchant_id: string | null
  epay_merchant_key: string | null
  epay_qr_enabled: boolean
  epay_refund_enabled: boolean
  epay_quota_per_cny: number
  epay_wxpay_enabled: boolean
  refund_auto_submit_enabled: boolean
  expected_version: number
  stripe_enabled: boolean
  stripe_publishable_key: string | null
  stripe_secret_key: string | null
  stripe_signature_tolerance_seconds: number
  stripe_webhook_secret: string | null
}

export type UserTopupMethod = {
  currency: string
  max_amount_minor: number
  min_amount_minor: number
  payment_method: PaymentMethod
  provider: PaymentProvider
  publishable_key?: string | null
  qr_enabled: boolean
}

export type UserTopupConfiguration = {
  methods: UserTopupMethod[]
}

export type UserTopupPayment =
  | {
      client_secret: string
      kind: 'stripe'
      payment_intent_id: string
    }
  | {
      redirect_url: string
      kind: 'redirect'
    }

export type UserTopupOrder = {
  amount_minor: number
  created_at: number
  currency: string
  order_id: string
  payment?: UserTopupPayment | null
  payment_method: PaymentMethod
  provider: PaymentProvider
  quota_amount: number
  replayed: boolean
  status: 'canceled' | 'created' | 'expired' | 'failed' | 'paid' | 'pending'
  version: number
}

export type UserTopupOrderCreateRequest = {
  amount_minor: number
  idempotency_key: string
  payment_method: PaymentMethod
  provider: PaymentProvider
}
