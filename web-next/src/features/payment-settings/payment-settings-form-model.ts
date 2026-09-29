import { z } from 'zod'

import type {
  AdminPaymentSettings,
  AdminPaymentSettingsRequest,
} from './payment-settings-types'

export type PaymentSettingsValues = {
  clearEpayMerchantKey: boolean
  clearStripeSecretKey: boolean
  clearStripeWebhookSecret: boolean
  epayAlipayEnabled: boolean
  epayEnabled: boolean
  epayGatewayUrl: string
  epayMerchantId: string
  epayMerchantKey: string
  epayQrEnabled: boolean
  epayRefundEnabled: boolean
  epayQuotaPerCny: number
  epayWxpayEnabled: boolean
  refundAutoSubmitEnabled: boolean
  stripeEnabled: boolean
  stripePublishableKey: string
  stripeSecretKey: string
  stripeSignatureToleranceSeconds: number
  stripeWebhookSecret: string
}

type ValidationMessages = {
  epayGatewayUrl: string
  epayMerchantId: string
  epayMerchantKey: string
  epayPaymentMethod: string
  epayRefund: string
  epayPublicBaseUrl: string
  epayQuota: string
  stripePublishableKey: string
  stripeSecretKey: string
  stripeTolerance: string
  stripeWebhookSecret: string
}

/** 从脱敏快照创建表单，所有服务端密钥都从空字符串开始。 */
export function paymentSettingsValues(settings: AdminPaymentSettings): PaymentSettingsValues {
  return {
    clearEpayMerchantKey: false,
    clearStripeSecretKey: false,
    clearStripeWebhookSecret: false,
    epayAlipayEnabled: settings.epay_alipay_enabled,
    epayEnabled: settings.epay_enabled,
    epayGatewayUrl: settings.epay_gateway_url ?? '',
    epayMerchantId: settings.epay_merchant_id ?? '',
    epayMerchantKey: '',
    epayQrEnabled: settings.epay_qr_enabled,
    epayRefundEnabled: settings.epay_refund_enabled,
    epayQuotaPerCny: settings.epay_quota_per_cny,
    epayWxpayEnabled: settings.epay_wxpay_enabled,
    refundAutoSubmitEnabled: settings.refund_auto_submit_enabled,
    stripeEnabled: settings.stripe_enabled,
    stripePublishableKey: settings.stripe_publishable_key ?? '',
    stripeSecretKey: '',
    stripeSignatureToleranceSeconds: settings.stripe_signature_tolerance_seconds,
    stripeWebhookSecret: '',
  }
}

/** 校验两个 Provider 的启用条件，并保留密钥替换语义。 */
export function buildPaymentSettingsSchema(
  settings: AdminPaymentSettings,
  messages: ValidationMessages,
  publicBaseUrl?: string | null,
) {
  return z.object({
    clearEpayMerchantKey: z.boolean(),
    clearStripeSecretKey: z.boolean(),
    clearStripeWebhookSecret: z.boolean(),
    epayAlipayEnabled: z.boolean(),
    epayEnabled: z.boolean(),
    epayGatewayUrl: z.string(),
    epayMerchantId: z.string(),
    epayMerchantKey: z.string(),
    epayQrEnabled: z.boolean(),
    epayRefundEnabled: z.boolean(),
    epayQuotaPerCny: z.number().int(messages.epayQuota).min(1, messages.epayQuota),
    epayWxpayEnabled: z.boolean(),
    refundAutoSubmitEnabled: z.boolean(),
    stripeEnabled: z.boolean(),
    stripePublishableKey: z.string(),
    stripeSecretKey: z.string(),
    stripeSignatureToleranceSeconds: z.number().int(messages.stripeTolerance).min(30, messages.stripeTolerance).max(900, messages.stripeTolerance),
    stripeWebhookSecret: z.string(),
  }).superRefine((values, context) => {
    if (values.clearStripeSecretKey && values.stripeSecretKey.trim()) {
      addIssue(context, 'stripeSecretKey', messages.stripeSecretKey)
    }
    if (values.clearStripeWebhookSecret && values.stripeWebhookSecret.trim()) {
      addIssue(context, 'stripeWebhookSecret', messages.stripeWebhookSecret)
    }
    if (values.stripePublishableKey && !validStripePublishableKey(values.stripePublishableKey)) {
      addIssue(context, 'stripePublishableKey', messages.stripePublishableKey)
    }
    if (values.stripeSecretKey && !validSecret(values.stripeSecretKey)) {
      addIssue(context, 'stripeSecretKey', messages.stripeSecretKey)
    }
    if (values.stripeWebhookSecret && !validSecret(values.stripeWebhookSecret)) {
      addIssue(context, 'stripeWebhookSecret', messages.stripeWebhookSecret)
    }
    if (values.stripeEnabled) {
      if (!values.stripePublishableKey) {
        addIssue(context, 'stripePublishableKey', messages.stripePublishableKey)
      }
      if ((!settings.stripe_secret_key_configured || values.clearStripeSecretKey) && !values.stripeSecretKey) {
        addIssue(context, 'stripeSecretKey', messages.stripeSecretKey)
      }
      if ((!settings.stripe_webhook_secret_configured || values.clearStripeWebhookSecret) && !values.stripeWebhookSecret) {
        addIssue(context, 'stripeWebhookSecret', messages.stripeWebhookSecret)
      }
    }

    if (values.epayGatewayUrl && !validHttpUrl(values.epayGatewayUrl)) {
      addIssue(context, 'epayGatewayUrl', messages.epayGatewayUrl)
    }
    if (values.epayMerchantId && !validMerchantId(values.epayMerchantId)) {
      addIssue(context, 'epayMerchantId', messages.epayMerchantId)
    }
    if (values.epayMerchantKey && !validSecret(values.epayMerchantKey)) {
      addIssue(context, 'epayMerchantKey', messages.epayMerchantKey)
    }
    if (values.clearEpayMerchantKey && values.epayMerchantKey.trim()) {
      addIssue(context, 'epayMerchantKey', messages.epayMerchantKey)
    }
    if (values.epayEnabled) {
      if (!publicBaseUrl) addIssue(context, 'epayEnabled', messages.epayPublicBaseUrl)
      if (!values.epayGatewayUrl) addIssue(context, 'epayGatewayUrl', messages.epayGatewayUrl)
      if (!values.epayMerchantId) addIssue(context, 'epayMerchantId', messages.epayMerchantId)
      if ((!settings.epay_merchant_key_configured || values.clearEpayMerchantKey) && !values.epayMerchantKey) {
        addIssue(context, 'epayMerchantKey', messages.epayMerchantKey)
      }
      if (!values.epayAlipayEnabled && !values.epayWxpayEnabled) {
        addIssue(context, 'epayAlipayEnabled', messages.epayPaymentMethod)
      }
    }
    if (values.epayRefundEnabled && !values.epayEnabled) {
      addIssue(context, 'epayRefundEnabled', messages.epayRefund)
    }
  })
}

export type PaymentCallbackUrls = {
  epay: string
  return: string
  stripe: string
}

/** 回调地址只从服务端公开基址派生，避免本地预览地址被误配到支付平台。 */
export function paymentCallbackUrls(publicBaseUrl?: string | null): PaymentCallbackUrls | undefined {
  if (!publicBaseUrl) return undefined
  try {
    const base = new URL(publicBaseUrl)
    base.search = ''
    base.hash = ''
    const prefix = base.pathname.replace(/\/+$/u, '')
    const callback = (path: string) => {
      const url = new URL(base)
      url.pathname = `${prefix}${path}`
      return url.toString()
    }
    const returnUrl = new URL(base)
    returnUrl.pathname = `${prefix}/console/wallet`
    returnUrl.searchParams.set('epay_return', '1')
    return {
      epay: callback('/api/payment/webhook/epay'),
      return: returnUrl.toString(),
      stripe: callback('/api/payment/webhook/stripe'),
    }
  } catch {
    return undefined
  }
}

/** 空密钥转换为 null，让服务端保留已有密文。 */
export function toPaymentSettingsRequest(
  values: PaymentSettingsValues,
  version: number,
): AdminPaymentSettingsRequest {
  return {
    clear_epay_merchant_key: values.clearEpayMerchantKey,
    clear_stripe_secret_key: values.clearStripeSecretKey,
    clear_stripe_webhook_secret: values.clearStripeWebhookSecret,
    epay_alipay_enabled: values.epayAlipayEnabled,
    epay_enabled: values.epayEnabled,
    epay_gateway_url: values.epayGatewayUrl || null,
    epay_merchant_id: values.epayMerchantId || null,
    epay_merchant_key: optionalSecret(values.epayMerchantKey),
    epay_qr_enabled: values.epayQrEnabled,
    epay_refund_enabled: values.epayRefundEnabled,
    epay_quota_per_cny: values.epayQuotaPerCny,
    epay_wxpay_enabled: values.epayWxpayEnabled,
    refund_auto_submit_enabled: values.refundAutoSubmitEnabled,
    expected_version: version,
    stripe_enabled: values.stripeEnabled,
    stripe_publishable_key: values.stripePublishableKey || null,
    stripe_secret_key: optionalSecret(values.stripeSecretKey),
    stripe_signature_tolerance_seconds: values.stripeSignatureToleranceSeconds,
    stripe_webhook_secret: optionalSecret(values.stripeWebhookSecret),
  }
}

/** 将全空白敏感输入归一为保留语义，避免意外覆盖已有密文。 */
function optionalSecret(value: string) {
  return value.trim() ? value : null
}

function validStripePublishableKey(value: string) {
  return value.length <= 512
    && value.trim() === value
    && (value.startsWith('pk_test_') || value.startsWith('pk_live_'))
    && printableAscii(value)
}

function validHttpUrl(value: string) {
  if (value.length > 2_048 || value.trim() !== value) return false
  try {
    const parsed = new URL(value)
    return parsed.protocol === 'https:'
      && Boolean(parsed.hostname)
      && !parsed.username
      && !parsed.password
      && !parsed.search
      && !parsed.hash
  } catch {
    return false
  }
}

function validMerchantId(value: string) {
  return value.length <= 128 && value.trim() === value && printableAscii(value)
}

function validSecret(value: string) {
  const bytes = new TextEncoder().encode(value).length
  return bytes > 0 && bytes <= 4_096 && ![...value].some((character) => /\p{Cc}/u.test(character))
}

function printableAscii(value: string) {
  return /^[\x21-\x7e]+$/u.test(value)
}

function addIssue(
  context: z.RefinementCtx,
  path: keyof PaymentSettingsValues,
  message: string,
) {
  context.addIssue({ code: 'custom', path: [path], message })
}
