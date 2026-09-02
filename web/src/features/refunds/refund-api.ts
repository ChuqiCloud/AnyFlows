import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  approveAdminRefund,
  listAdminRefunds,
  manualCompleteAdminRefund,
  rejectAdminRefund,
  submitAdminRefund,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminRefundListResponse,
  AdminRefundManualCompletionRequest,
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
    // 无论成功、CAS 冲突还是结果未知，都重新读取服务端版本，避免继续使用过期审批事实。
    onSettled: () => queryClient.invalidateQueries({ queryKey: adminRefundsQueryKey }),
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

export function useManualCompleteAdminRefund() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: {
      requestId: string
      body: AdminRefundManualCompletionRequest
    }) => {
      const { data } = await manualCompleteAdminRefund({
        client: apiClient,
        path: { request_id: input.requestId },
        body: input.body,
        throwOnError: true as const,
      })
      return data
    },
    // 结果未知或 CAS 冲突时也要刷新，确保界面显示服务端的最新版本。
    onSettled: () => queryClient.invalidateQueries({ queryKey: adminRefundsQueryKey }),
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
