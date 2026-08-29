import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient, ApiError } from '@/lib/api'
import {
  closeOrganizationContractPrice,
  createOrganizationContractPrice,
  getOrganizationContractPrice,
  listOrganizationContractPrices,
} from '@/lib/api/generated/sdk.gen'
import type {
  ContractPriceCloseRequest,
  ContractPriceCreateRequest,
} from '@/lib/api/generated/types.gen'

/** 读取当前企业的不可变合同价版本，权限由企业概览投影决定。 */
export function useOrganizationContractPrices(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'contract-prices'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationContractPrices({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data
    },
  })
}

/** 读取单个合同价详情，供冲突后按服务端事实复核。 */
export function useOrganizationContractPrice(organizationId: number, priceId: number | undefined, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'contract-price', priceId),
    enabled: enabled && priceId !== undefined,
    queryFn: async ({ signal }) => {
      const { data } = await getOrganizationContractPrice({
        client: apiClient,
        path: { organization_id: organizationId, price_id: priceId as number },
        signal,
      })
      return data
    },
  })
}

/** 创建合同价版本后刷新列表；不做乐观写入，避免覆盖并发版本。 */
export function useCreateOrganizationContractPrice(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: ContractPriceCreateRequest) => {
      const { data } = await createOrganizationContractPrice({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateContractPrices(queryClient, organizationId),
  })
}

/** 关闭合同价区间时携带服务端版本，冲突保持失败关闭。 */
export function useCloseOrganizationContractPrice(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ priceId, body }: { priceId: number; body: ContractPriceCloseRequest }) => {
      const { data } = await closeOrganizationContractPrice({
        body,
        client: apiClient,
        path: { organization_id: organizationId, price_id: priceId },
      })
      return data
    },
    onSuccess: () => invalidateContractPrices(queryClient, organizationId),
  })
}

/** 提取合同价接口的公开错误码，不把服务端敏感详情直接展示给用户。 */
export function organizationContractPriceErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}

export function organizationContractPriceErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}

function invalidateContractPrices(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'contract-prices') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') }),
  ])
}
