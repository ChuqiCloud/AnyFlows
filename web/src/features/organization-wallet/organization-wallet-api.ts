import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient, ApiError } from '@/lib/api'
import {
  adjustOrganizationWallet,
  createOrganizationTopupOrder,
  getOrganizationWallet,
  listOrganizationWalletEntries,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationTopupOrderCreateRequest,
  OrganizationWalletAdjustmentRequest,
} from '@/lib/api/generated/types.gen'

/** 读取当前企业钱包摘要；权限不足时由页面展示服务端裁剪结果。 */
export function useOrganizationWalletSummary(organizationId: number, enabled = true) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'wallet', 'summary'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await getOrganizationWallet({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data
    },
    retry: false,
    refetchInterval: (query) => query.state.status === 'error' ? false : 30_000,
  })
}

/** 按不可变账本 ID 倒序读取企业资金事实。 */
export function useOrganizationWalletEntries(organizationId: number, enabled = true) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'wallet', 'entries'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationWalletEntries({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { before: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}

/** 调账失败时保留调用方事件键；成功后统一校正摘要和账本。 */
export function useAdjustOrganizationWallet(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationWalletAdjustmentRequest) => {
      const { data } = await adjustOrganizationWallet({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationWallet(queryClient, organizationId),
  })
}

/** 企业充值订单与个人订单使用不同服务端主体和浏览器幂等命名空间。 */
export function useCreateOrganizationTopupOrder(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async (body: OrganizationTopupOrderCreateRequest) => {
      const { data } = await createOrganizationTopupOrder({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: (order) => order.status === 'paid'
      ? invalidateOrganizationWallet(queryClient, organizationId)
      : undefined,
  })
}

export function organizationWalletErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}

function invalidateOrganizationWallet(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'wallet', 'summary') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'wallet', 'entries') }),
  ])
}
