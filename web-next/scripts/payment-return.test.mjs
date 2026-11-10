import assert from 'node:assert/strict'
import test from 'node:test'

import {
  cleanSubscriptionPaymentReturnUrl,
  cleanWalletPaymentReturnUrl,
  createSubscriptionPaymentReturnUrl,
  browserRouterBasename,
  browserRouterPath,
  createTopupPaymentReturnUrl,
  migrateLegacyEpayHashReturn,
  subscriptionPaymentRedirectStatus,
  topupPaymentRedirectStatus,
} from '../src/features/wallet/payment-return-model.ts'
import { isCurrentTopupAttempt, parseStoredTopupAttempt } from '../src/features/wallet/topup-form-model.ts'
import { parseStoredSubscriptionAttempt, submitCreatedSubscriptionPayment } from '../src/features/subscriptions/subscription-payment-model.ts'
import { paymentCallbackUrls } from '../src/features/payment-settings/payment-settings-form-model.ts'

const stripeAttempt = { provider: 'stripe', orderId: 'topup-1', paymentIntentId: 'pi-1' }

test('Stripe topup requires matching flow, order and PaymentIntent', () => {
  const valid = new URL('https://example.test/console/wallet?flow=topup&topup_order_id=topup-1&topup_payment_intent=pi-1&payment_intent=pi-1&redirect_status=succeeded')
  assert.equal(topupPaymentRedirectStatus(valid, stripeAttempt), 'succeeded')
  assert.equal(topupPaymentRedirectStatus(new URL(valid.href.replace('topup-1', 'other')), stripeAttempt), undefined)
  assert.equal(topupPaymentRedirectStatus(new URL(valid.href.replace('pi-1', 'pi-other')), stripeAttempt), undefined)
  assert.equal(topupPaymentRedirectStatus(new URL(valid.href.replace('flow=topup', 'flow=subscription')), stripeAttempt), undefined)
  const wrongReturnedIntent = new URL(valid)
  wrongReturnedIntent.searchParams.set('payment_intent', 'pi-other')
  assert.equal(topupPaymentRedirectStatus(wrongReturnedIntent, stripeAttempt), undefined)
  for (const name of ['flow', 'topup_order_id', 'topup_payment_intent', 'payment_intent', 'redirect_status']) {
    const missing = new URL(valid)
    missing.searchParams.delete(name)
    assert.equal(topupPaymentRedirectStatus(missing, stripeAttempt), undefined, name)
  }
  const ambiguous = new URL(valid)
  ambiguous.hash = '#/console/wallet?topup_order_id=topup-1'
  assert.equal(topupPaymentRedirectStatus(ambiguous, stripeAttempt), undefined)
  const conflictingFlow = new URL(valid)
  conflictingFlow.hash = '#/console/wallet?flow=subscription'
  assert.equal(topupPaymentRedirectStatus(conflictingFlow, stripeAttempt), undefined)
  const repeatedFlow = new URL(valid)
  repeatedFlow.searchParams.append('flow', 'topup')
  assert.equal(topupPaymentRedirectStatus(repeatedFlow, stripeAttempt), undefined)
})

test('subscription parameters remain available to subscription flow', () => {
  const callback = new URL('https://example.test/console/wallet?flow=subscription&subscription_order_id=sub-1&subscription_payment_intent=pi-sub&payment_intent=pi-sub&payment_intent_client_secret=secret&redirect_status=processing')
  const attempt = { provider: 'stripe', orderId: 'sub-1', paymentIntentId: 'pi-sub' }
  assert.equal(subscriptionPaymentRedirectStatus(callback, attempt), 'processing')
  assert.equal(topupPaymentRedirectStatus(callback, stripeAttempt), undefined)
  const walletUrl = cleanWalletPaymentReturnUrl(callback)
  assert.match(walletUrl.search, /subscription_order_id=sub-1/)
  assert.doesNotMatch(walletUrl.search, /payment_intent_client_secret/)
  const cleanedSubscription = cleanSubscriptionPaymentReturnUrl(callback)
  assert.equal(cleanedSubscription.pathname, '/console/wallet')
  assert.equal(cleanedSubscription.search, '')
  assert.equal(subscriptionPaymentRedirectStatus(walletUrl, attempt), 'processing')
  assert.equal(subscriptionPaymentRedirectStatus(callback, { ...attempt, orderId: 'sub-other' }), undefined)
  assert.equal(subscriptionPaymentRedirectStatus(callback, { ...attempt, paymentIntentId: 'pi-other' }), undefined)
  assert.equal(callback.searchParams.get('payment_intent_client_secret'), 'secret', 'cleanup must not mutate its input')
  const hashCallback = new URL('https://example.test/#/console/wallet?flow=subscription&subscription_order_id=sub-1&subscription_payment_intent=pi-sub&payment_intent=pi-sub&payment_intent_client_secret=secret&redirect_status=processing')
  const hashWalletUrl = cleanWalletPaymentReturnUrl(hashCallback)
  assert.equal(subscriptionPaymentRedirectStatus(hashWalletUrl, attempt), 'processing')
  assert.doesNotMatch(hashWalletUrl.hash, /payment_intent_client_secret/)
  assert.equal(cleanSubscriptionPaymentReturnUrl(hashWalletUrl).hash, '#/console/wallet')
})

test('Epay requires exact out_trade_no and supports hash callbacks', () => {
  const valid = new URL('https://example.test/#/console/wallet?out_trade_no=topup-1&trade_no=trade-1&trade_status=TRADE_SUCCESS')
  assert.equal(topupPaymentRedirectStatus(valid, { provider: 'epay', orderId: 'topup-1' }), 'processing')
  assert.equal(topupPaymentRedirectStatus(new URL(valid.href.replace('topup-1', 'topup-2')), { provider: 'epay', orderId: 'topup-1' }), undefined)
  const subscriptionUrl = new URL('https://example.test/console/wallet?flow=subscription&out_trade_no=topup-1&trade_no=trade-1')
  assert.equal(topupPaymentRedirectStatus(subscriptionUrl, { provider: 'epay', orderId: 'topup-1' }), undefined)
  const mixedFlow = new URL('https://example.test/?flow=subscription#/console/wallet?flow=topup&epay_return=1&out_trade_no=topup-1')
  assert.equal(topupPaymentRedirectStatus(mixedFlow, { provider: 'epay', orderId: 'topup-1' }), undefined)
  const fixedCallback = new URL('https://example.test/?out_trade_no=topup-1&trade_no=trade-1#/console/wallet?epay_return=1&tab=billing')
  assert.equal(topupPaymentRedirectStatus(fixedCallback, { provider: 'epay', orderId: 'topup-1' }), 'processing')
  assert.equal(cleanWalletPaymentReturnUrl(fixedCallback).href, 'https://example.test/#/console/wallet?tab=billing')
  assert.equal(topupPaymentRedirectStatus(new URL('https://example.test/?epay_return=1&out_trade_no=topup-10'), { provider: 'epay', orderId: 'topup-1' }), undefined)
})

test('legacy Epay hash return migrates to the BrowserRouter wallet path', () => {
  const legacy = new URL('https://example.test/gateway/#/console/wallet?epay_return=1&out_trade_no=topup-1&trade_no=trade-1&trade_status=TRADE_SUCCESS')
  const migrated = migrateLegacyEpayHashReturn(legacy)
  assert.equal(migrated.pathname, '/gateway/console/wallet')
  assert.equal(migrated.hash, '')
  assert.equal(browserRouterBasename(migrated.pathname), '/gateway')
  assert.equal(topupPaymentRedirectStatus(migrated, { provider: 'epay', orderId: 'topup-1' }), 'processing')
  assert.equal(migrateLegacyEpayHashReturn(new URL('https://example.test/#/console/wallet?tab=billing')).hash, '#/console/wallet?tab=billing')
  const conflicting = migrateLegacyEpayHashReturn(new URL('https://example.test/?out_trade_no=other#/console/wallet?epay_return=1&out_trade_no=topup-1'))
  assert.equal(topupPaymentRedirectStatus(conflicting, { provider: 'epay', orderId: 'topup-1' }), undefined)
})

test('late topup responses only apply to the current idempotent attempt', () => {
  const first = { idempotencyKey: 'a'.repeat(32) }
  const replacement = { idempotencyKey: 'b'.repeat(32) }
  assert.equal(isCurrentTopupAttempt(first, first), true)
  assert.equal(isCurrentTopupAttempt(replacement, first), false)
  assert.equal(isCurrentTopupAttempt(undefined, first), false)
})

test('old callback parameters and malformed storage do not restore a payment', () => {
  assert.equal(topupPaymentRedirectStatus(new URL('https://example.test/console/wallet?payment_intent=pi-1&redirect_status=succeeded'), stripeAttempt), undefined)
  const bounds = { min: 50, max: 99_999_999 }
  assert.equal(parseStoredTopupAttempt('{"idempotencyKey":"bad","amountMinor":100,"provider":"stripe","paymentMethod":"card"}', bounds), undefined)
  assert.equal(parseStoredSubscriptionAttempt('{"idempotencyKey":"bad","planId":"plan-1","provider":"stripe","paymentMethod":"card"}', { plan_id: 'plan-1', price_provider: 'stripe' }), undefined)
})

test('storage parser rejects corrupted records and preserves valid correlation', () => {
  const bounds = { min: 50, max: 99_999_999 }
  const topup = { ...stripeAttempt, idempotencyKey: 'a'.repeat(32), amountMinor: 100, paymentMethod: 'card' }
  const subscription = { ...stripeAttempt, idempotencyKey: 'a'.repeat(32), planId: 'plan-1', paymentMethod: 'card' }
  const plan = { plan_id: 'plan-1', price_provider: 'stripe' }
  for (const invalid of [null, 'null', '[]', 'not json', '"text"']) {
    assert.equal(parseStoredTopupAttempt(invalid, bounds), undefined)
    assert.equal(parseStoredSubscriptionAttempt(invalid, plan), undefined)
  }
  for (const invalidId of ['', ' padded ', 12, null, {}]) {
    assert.equal(parseStoredTopupAttempt(JSON.stringify({ ...topup, orderId: invalidId }), bounds), undefined)
    assert.equal(parseStoredSubscriptionAttempt(JSON.stringify({ ...subscription, paymentIntentId: invalidId }), plan), undefined)
  }
  assert.deepEqual(parseStoredTopupAttempt(JSON.stringify(topup), bounds), topup)
  assert.deepEqual(parseStoredSubscriptionAttempt(JSON.stringify(subscription), plan), subscription)
  assert.equal(parseStoredSubscriptionAttempt(JSON.stringify(subscription), { ...plan, plan_id: 'plan-other' }), undefined)
  assert.equal(parseStoredSubscriptionAttempt(JSON.stringify(subscription), { ...plan, price_provider: 'epay' }), undefined)
})

test('new return URLs replace stale flow material while preserving the destination', () => {
  const topup = createTopupPaymentReturnUrl(new URL('https://example.test/console/wallet?flow=subscription&subscription_order_id=old&tab=billing&payment_intent_client_secret=secret'), '/console/wallet', stripeAttempt)
  assert.equal(topup.pathname, '/console/wallet')
  assert.equal(topup.searchParams.get('tab'), 'billing')
  assert.equal(topup.searchParams.has('subscription_order_id'), false)
  assert.equal(topup.searchParams.has('payment_intent_client_secret'), false)
  topup.searchParams.set('payment_intent', stripeAttempt.paymentIntentId)
  topup.searchParams.set('redirect_status', 'succeeded')
  assert.equal(topupPaymentRedirectStatus(topup, stripeAttempt), 'succeeded')
  const subscription = createSubscriptionPaymentReturnUrl('https://example.test', 'sub-1', 'pi-sub')
  assert.equal(subscription.pathname, '/console/wallet')
  subscription.searchParams.set('payment_intent', 'pi-sub')
  subscription.searchParams.set('redirect_status', 'succeeded')
  assert.equal(subscriptionPaymentRedirectStatus(subscription, { provider: 'stripe', orderId: 'sub-1', paymentIntentId: 'pi-sub' }), 'succeeded')
  const prefixedTopup = createTopupPaymentReturnUrl(new URL('https://example.test/gateway/console/wallet'), browserRouterPath('/gateway/console/wallet', '/console/wallet'), stripeAttempt)
  const prefixedSubscription = createSubscriptionPaymentReturnUrl('https://example.test', 'sub-1', 'pi-sub', browserRouterPath('/gateway/console/wallet', '/console/wallet'))
  assert.equal(prefixedTopup.pathname, '/gateway/console/wallet')
  assert.equal(prefixedSubscription.pathname, '/gateway/console/wallet')
  assert.equal(browserRouterBasename('/console/wallet'), '')
})

test('Epay settings display the BrowserRouter return path', () => {
  const callbacks = paymentCallbackUrls('https://example.test/gateway/')
  assert.equal(callbacks?.epay, 'https://example.test/gateway/api/payment/webhook/epay')
  assert.equal(callbacks?.return, 'https://example.test/gateway/console/wallet?epay_return=1')
})

test('a created order remains recoverable when preparing payment fails', async () => {
  const attempt = { idempotencyKey: 'a'.repeat(32), planId: 'plan-1', provider: 'stripe', paymentMethod: 'card' }
  let stored
  await assert.rejects(submitCreatedSubscriptionPayment(attempt, 'sub-created', (next) => { stored = next }, async () => {
    assert.equal(stored.orderId, 'sub-created', 'order must be saved before payment starts')
    throw new Error('payment unavailable')
  }), /payment unavailable/)
  assert.deepEqual(stored, { ...attempt, orderId: 'sub-created' })
  assert.equal(attempt.orderId, undefined, 'the original draft remains unchanged')
})
