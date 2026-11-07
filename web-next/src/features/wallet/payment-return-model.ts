export type PaymentConfirmationPhase = 'succeeded' | 'processing' | 'failed'

type PaymentReturnAttempt = {
  provider: string
  orderId?: string
  paymentIntentId?: string
}

const stripeParameters = ['payment_intent', 'payment_intent_client_secret', 'redirect_status'] as const
const topupParameters = ['topup_order_id', 'topup_payment_intent'] as const
const subscriptionParameters = ['subscription_order_id', 'subscription_payment_intent'] as const
const epayParameters = ['epay_return', 'pid', 'trade_no', 'out_trade_no', 'type', 'name', 'money', 'trade_status', 'sign', 'sign_type'] as const

/** 从当前控制台路径推导部署前缀，供路由器和支付回跳地址共用。 */
export function browserRouterBasename(pathname: string) {
  const segments = pathname.split('/').filter(Boolean)
  const consoleIndex = segments.indexOf('console')
  return consoleIndex > 0 ? `/${segments.slice(0, consoleIndex).join('/')}` : ''
}

export function browserRouterPath(pathname: string, routePath: string) {
  const basename = browserRouterBasename(pathname)
  return `${basename}${routePath.startsWith('/') ? routePath : `/${routePath}`}`
}

/** 同时读取服务商 query 和现有易支付 hash 回跳，保留重复字段供匹配时拒绝歧义。 */
function returnParameters(url: URL) {
  const parameters = new URLSearchParams(url.search)
  const hashQuery = url.hash.indexOf('?')
  if (hashQuery >= 0) {
    new URLSearchParams(url.hash.slice(hashQuery + 1)).forEach((value, key) => parameters.append(key, value))
  }
  return parameters
}

function singleParameter(parameters: URLSearchParams, key: string) {
  const values = parameters.getAll(key)
  return values.length === 1 ? values[0] : undefined
}

function matchesStripeReturn(parameters: URLSearchParams, attempt: PaymentReturnAttempt, flow: 'topup' | 'subscription') {
  if (!attempt.orderId || !attempt.paymentIntentId || attempt.provider !== 'stripe') return false
  return singleParameter(parameters, 'flow') === flow
    && singleParameter(parameters, `${flow}_order_id`) === attempt.orderId
    && singleParameter(parameters, `${flow}_payment_intent`) === attempt.paymentIntentId
    && singleParameter(parameters, 'payment_intent') === attempt.paymentIntentId
}

/** URL 状态仅触发恢复；到账事实始终从服务端订单读取。 */
export function topupPaymentRedirectStatus(url: URL, attempt: PaymentReturnAttempt | undefined): PaymentConfirmationPhase | undefined {
  if (!attempt?.orderId) return undefined
  const parameters = returnParameters(url)
  if (parameters.getAll('flow').includes('subscription')) return undefined
  if (attempt.provider === 'epay') {
    if (singleParameter(parameters, 'out_trade_no') !== attempt.orderId) return undefined
    return epayParameters.some((parameter) => parameter !== 'out_trade_no' && parameters.has(parameter)) ? 'processing' : undefined
  }
  if (!matchesStripeReturn(parameters, attempt, 'topup')) return undefined
  const status = singleParameter(parameters, 'redirect_status')
  return status === 'succeeded' || status === 'processing' || status === 'failed' ? status : undefined
}

export function subscriptionPaymentRedirectStatus(url: URL, attempt: PaymentReturnAttempt | undefined): PaymentConfirmationPhase | undefined {
  if (!attempt) return undefined
  const parameters = returnParameters(url)
  if (!matchesStripeReturn(parameters, attempt, 'subscription')) return undefined
  const status = singleParameter(parameters, 'redirect_status')
  return status === 'succeeded' || status === 'processing' || status === 'failed' ? status : undefined
}

/** 清理支付材料时保留其他路由参数，钱包让订阅组件消费自己的关联字段。 */
export function cleanWalletPaymentReturnUrl(input: URL) {
  const url = new URL(input)
  if (returnParameters(url).getAll('flow').includes('subscription')) {
    removeParameters(url, ['payment_intent_client_secret'])
    return url
  }
  const ownFlow = returnParameters(url).getAll('flow').includes('topup') ? ['flow'] : []
  removeParameters(url, [...ownFlow, ...topupParameters, ...stripeParameters, ...epayParameters])
  return url
}

export function cleanSubscriptionPaymentReturnUrl(input: URL) {
  const url = new URL(input)
  removeParameters(url, ['flow', ...subscriptionParameters, ...stripeParameters])
  return url
}

export function createTopupPaymentReturnUrl(input: URL, returnPath: string, attempt: { orderId?: string; paymentIntentId?: string }) {
  const url = new URL(input)
  removeParameters(url, ['flow', ...topupParameters, ...subscriptionParameters, ...stripeParameters, ...epayParameters])
  url.pathname = returnPath
  url.searchParams.set('flow', 'topup')
  if (attempt.orderId) url.searchParams.set('topup_order_id', attempt.orderId)
  if (attempt.paymentIntentId) url.searchParams.set('topup_payment_intent', attempt.paymentIntentId)
  return url
}

export function createSubscriptionPaymentReturnUrl(origin: string, orderId: string, paymentIntentId: string, returnPath = '/console/wallet') {
  const url = new URL(returnPath, origin)
  url.searchParams.set('flow', 'subscription')
  url.searchParams.set('subscription_order_id', orderId)
  url.searchParams.set('subscription_payment_intent', paymentIntentId)
  return url
}

/** Move callbacks created by the previous hash-based Epay URL onto the BrowserRouter path. */
export function migrateLegacyEpayHashReturn(input: URL) {
  const url = new URL(input)
  const match = /^#(\/console\/wallet)(?:\?(.*))?$/u.exec(url.hash)
  if (!match) return url

  const hashParameters = new URLSearchParams(match[2] ?? '')
  if (!hashParameters.has('epay_return') && !hashParameters.has('out_trade_no')) return url

  const parameters = new URLSearchParams(url.search)
  hashParameters.forEach((value, key) => {
    if (!parameters.getAll(key).includes(value)) parameters.append(key, value)
  })
  const currentPath = url.pathname.replace(/\/+$/u, '')
  const prefix = currentPath.endsWith(match[1])
    ? currentPath.slice(0, -match[1].length)
    : currentPath
  url.pathname = `${prefix}${match[1]}`
  url.search = parameters.toString()
  url.hash = ''
  return url
}

function removeParameters(url: URL, names: readonly string[]) {
  for (const parameter of names) url.searchParams.delete(parameter)
  const hashQuery = url.hash.indexOf('?')
  if (hashQuery < 0) return
  const prefix = url.hash.slice(0, hashQuery)
  const parameters = new URLSearchParams(url.hash.slice(hashQuery + 1))
  for (const parameter of names) parameters.delete(parameter)
  const query = parameters.toString()
  url.hash = query ? `${prefix}?${query}` : prefix
}
