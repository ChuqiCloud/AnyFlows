import { z } from 'zod'

import type { AdminWalletAdjustmentRequest } from '@/lib/api/generated/types.gen'

export type WalletAdjustmentDirection = 'increase' | 'decrease'

export type WalletAdjustmentValues = {
  direction: WalletAdjustmentDirection
  amount: string
  reason: string
}

export type WalletAdjustmentAttempt = {
  eventId: string
  fingerprint: string
}

type WalletValidationMessages = {
  invalidAmount: string
  invalidReason: string
}

const SYSTEM_OPENING_PREFIX = '0000000000000001'
const MAX_REASON_BYTES = 500

export function defaultWalletAdjustmentValues(): WalletAdjustmentValues {
  return { direction: 'increase', amount: '', reason: '' }
}

/** 复现服务端调账输入边界，并把金额限制在 JavaScript 可精确表示范围内。 */
export function buildWalletAdjustmentSchema(messages: WalletValidationMessages) {
  return z.object({
    direction: z.enum(['increase', 'decrease']),
    amount: z.string().refine((value) => parseWalletAmount(value) !== undefined, messages.invalidAmount),
    reason: z.string().refine(isValidWalletReason, messages.invalidReason),
  })
}

/** 生成非零且不占用 opening 命名空间的 128 位小写十六进制事件键。 */
export function createWalletEventId() {
  const bytes = new Uint8Array(16)
  let eventId = ''
  do {
    globalThis.crypto.getRandomValues(bytes)
    eventId = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
  } while (/^0{32}$/.test(eventId) || eventId.startsWith(SYSTEM_OPENING_PREFIX))
  return eventId
}

export function createWalletAdjustmentAttempt(values: WalletAdjustmentValues): WalletAdjustmentAttempt {
  return {
    eventId: createWalletEventId(),
    fingerprint: walletAdjustmentFingerprint(values),
  }
}

/** 字段不变时保留事件键供安全重试，业务事实变化后才创建新键。 */
export function walletAdjustmentAttemptForValues(
  attempt: WalletAdjustmentAttempt,
  values: WalletAdjustmentValues,
) {
  const fingerprint = walletAdjustmentFingerprint(values)
  return attempt.fingerprint === fingerprint
    ? attempt
    : { eventId: createWalletEventId(), fingerprint }
}

export function toWalletAdjustmentRequest(
  values: WalletAdjustmentValues,
  eventId: string,
): AdminWalletAdjustmentRequest {
  const amount = parseWalletAmount(values.amount)
  if (amount === undefined || !isValidWalletReason(values.reason)) {
    throw new Error('钱包调账表单未通过校验')
  }
  return {
    event_id: eventId,
    quota_delta: values.direction === 'increase' ? amount : -amount,
    reason: values.reason,
  }
}

export function previewWalletBalance(
  balance: number,
  direction: WalletAdjustmentDirection,
  amountText: string,
): { status: 'empty' | 'insufficient' | 'overflow' | 'ready'; balance?: number } {
  const amount = parseWalletAmount(amountText)
  if (amount === undefined) return { status: 'empty' }
  if (direction === 'decrease') {
    return amount > balance
      ? { status: 'insufficient' }
      : { status: 'ready', balance: balance - amount }
  }
  const next = balance + amount
  return Number.isSafeInteger(next)
    ? { status: 'ready', balance: next }
    : { status: 'overflow' }
}

function parseWalletAmount(value: string) {
  if (!/^[1-9][0-9]*$/.test(value)) return undefined
  const amount = Number(value)
  return Number.isSafeInteger(amount) ? amount : undefined
}

function isValidWalletReason(value: string) {
  return value.length > 0
    && value.trim() === value
    && new TextEncoder().encode(value).length <= MAX_REASON_BYTES
    && ![...value].some((character) => /\p{Cc}/u.test(character))
}

function walletAdjustmentFingerprint(values: WalletAdjustmentValues) {
  return JSON.stringify(values)
}
