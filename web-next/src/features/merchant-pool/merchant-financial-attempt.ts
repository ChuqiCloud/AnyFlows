import { poolEventKey } from './merchant-pool-model.ts'

const attempts = new Map<string, { fingerprint: string; eventKey: string }>()

/** Keep a stable financial operation key after an uncertain response, including when storage is unavailable. */
export function merchantFinancialAttempt(storageKey: string, fingerprint: string) {
  let attempt = attempts.get(storageKey)
  if (!attempt) {
    try {
      const stored: unknown = JSON.parse(sessionStorage.getItem(storageKey) ?? 'null')
      if (stored && typeof stored === 'object' && 'fingerprint' in stored && typeof stored.fingerprint === 'string' && 'eventKey' in stored && typeof stored.eventKey === 'string' && /^[0-9a-f]{32}$/.test(stored.eventKey)) attempt = { fingerprint: stored.fingerprint, eventKey: stored.eventKey }
    } catch { /* Component memory remains available when session storage is blocked. */ }
  }
  if (!attempt || attempt.fingerprint !== fingerprint) attempt = { fingerprint, eventKey: poolEventKey() }
  attempts.set(storageKey, attempt)
  try { sessionStorage.setItem(storageKey, JSON.stringify(attempt)) } catch { /* Keep the in-memory attempt. */ }
  return attempt.eventKey
}

export function clearMerchantFinancialAttempt(storageKey: string) {
  attempts.delete(storageKey)
  try { sessionStorage.removeItem(storageKey) } catch { /* The operation already completed. */ }
}
