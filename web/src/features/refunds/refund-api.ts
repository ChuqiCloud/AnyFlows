import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  approveAdminRefund,
  listAdminRefunds,
  rejectAdminRefund,
  submitAdminRefund,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminRefundListResponse,
  AdminRefundRequest,
} from '@/lib/api/generated/types.gen'

export const adminRefundsQueryKey = ['admin-refunds'] as const

/** 按服务端 ID 游标读取退款审批事实，不在浏览器端拼接或排序金额。 */
export function useAdminRefunds(approvalStatus: string | undefined, pageSize = 30) {
  return useInfiniteQuery({
    queryKey: [...adminRefundsQueryKey, { approvalStatus, pageSize }],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminRefunds({
        client: apiClient,
        query: { after: pageParam, approval_status: approvalStatus, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

function useRefundAction(action: 'approve' | 'reject') {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ requestId, reason }: { requestId: string; reason?: string }) => {
      const options = {
        client: apiClient,
        path: { request_id: requestId },
        body: { reason: reason?.trim() || null },
        throwOnError: true as const,
      }
      return action === 'approve'
        ? (await approveAdminRefund(options)).data
        : (await rejectAdminRefund(options)).data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminRefundsQueryKey }),
  })
}

export function useApproveAdminRefund() {
  return useRefundAction('approve')
}

export function useRejectAdminRefund() {
  return useRefundAction('reject')
}

export function useSubmitAdminRefund() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (requestId: string) => {
      const { data } = await submitAdminRefund({
        client: apiClient,
        path: { request_id: requestId },
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminRefundsQueryKey }),
  })
}

export function refundErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}

export function flattenRefundPages(pages: AdminRefundListResponse[] | undefined): AdminRefundRequest[] {
  return pages?.flatMap((page) => page.entries) ?? []
}
