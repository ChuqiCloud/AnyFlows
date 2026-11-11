import { ApiError } from '@/lib/api'

export function merchantPoolErrorCode(error: unknown): string | undefined {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const details = error.details
  if ('code' in details && typeof details.code === 'string') return details.code
  if (!('error' in details) || typeof details.error !== 'object' || details.error === null) return undefined
  return 'code' in details.error && typeof details.error.code === 'string' ? details.error.code : undefined
}
