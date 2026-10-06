import type { AdminRedemptionAuditStatus } from '@/lib/api/generated/types.gen'

export type RedemptionAuditStatusFilter = AdminRedemptionAuditStatus | 'all'

export type RedemptionAuditFilterDraft = {
  status: RedemptionAuditStatusFilter
  batchId: string
  redeemedAfter: string
  redeemedBefore: string
}

export type RedemptionAuditFilters = {
  status?: AdminRedemptionAuditStatus
  batchId?: string
  redeemedAfter?: number
  redeemedBefore?: number
}

export type RedemptionAuditFilterError = 'invalidBatchId' | 'invalidDate' | 'reversed'

export type RedemptionAuditFilterResult =
  | { ok: true; filters: RedemptionAuditFilters }
  | { ok: false; error: RedemptionAuditFilterError }

/** 把表单草稿收敛为后端接受的精确筛选条件，非法草稿不会触发网络请求。 */
export function parseRedemptionAuditFilters(
  draft: RedemptionAuditFilterDraft,
): RedemptionAuditFilterResult {
  const batchId = draft.batchId.trim().toLowerCase()
  if (batchId !== '' && !/^[0-9a-f]{32}$/.test(batchId)) {
    return { ok: false, error: 'invalidBatchId' }
  }

  const redeemedAfter = parseDateInput(draft.redeemedAfter)
  const redeemedBefore = parseDateInput(draft.redeemedBefore)
  if ((draft.redeemedAfter !== '' && redeemedAfter === undefined)
    || (draft.redeemedBefore !== '' && redeemedBefore === undefined)) {
    return { ok: false, error: 'invalidDate' }
  }
  if (redeemedAfter !== undefined
    && redeemedBefore !== undefined
    && redeemedAfter >= redeemedBefore) {
    return { ok: false, error: 'reversed' }
  }

  return {
    ok: true,
    filters: {
      status: draft.status === 'all' ? undefined : draft.status,
      batchId: batchId || undefined,
      redeemedAfter,
      redeemedBefore,
    },
  }
}

function parseDateInput(value: string): number | undefined {
  if (!value) return undefined
  const milliseconds = Date.parse(value)
  if (!Number.isFinite(milliseconds) || milliseconds <= 0) return undefined
  const seconds = Math.floor(milliseconds / 1_000)
  return Number.isSafeInteger(seconds) && seconds > 0 ? seconds : undefined
}
