import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createAdminOrganizationPlan,
  createOrganizationPlanOrder,
  disableAdminOrganizationPlan,
  getOrganizationPlanOrder,
  getOrganizationPlanCatalog,
  listAdminOrganizationPlans,
  submitOrganizationPlanOrderPayment,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationPlanOrderCreateRequest,
  OrganizationPlanOrderPaymentRequest,
  OrganizationPlanCreateRequest,
  OrganizationPlanDisableRequest,
} from '@/lib/api/generated/types.gen'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'

export const organizationPlanCatalogQueryKey = ['organization-plan-catalog'] as const
export const adminOrganizationPlansQueryKey = ['admin-organization-plans'] as const

/** 读取当前账号可售的企业套餐快照；支付和授权发放仍由后续商业生命周期接入。 */
export function useOrganizationPlanCatalog() {
  return useQuery({
    queryKey: organizationPlanCatalogQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getOrganizationPlanCatalog({ client: apiClient, signal })
      return data
    },
    retry: false,
  })
}

/** 在路径企业内创建或恢复当前付款人的授权订单。 */
export function useCreateOrganizationPlanOrder(organizationId: number) {
  return useMutation({
    mutationFn: async (body: OrganizationPlanOrderCreateRequest) => {
      const { data } = await createOrganizationPlanOrder({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
  })
}

/** 按企业和订单双重范围读取服务端支付状态。 */
export function useOrganizationPlanOrder(organizationId: number, orderId: string | undefined) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'plan-orders', orderId),
    enabled: orderId !== undefined,
    queryFn: async ({ signal }) => {
      if (orderId === undefined) throw new Error('缺少企业授权订单 ID')
      const { data } = await getOrganizationPlanOrder({
        client: apiClient,
        path: { organization_id: organizationId, order_id: orderId },
        signal,
      })
      return data
    },
    retry: false,
  })
}

/** 复验企业钱包管理权限后提交或恢复同一订单的支付会话。 */
export function useSubmitOrganizationPlanOrderPayment(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ orderId, body }: { orderId: string; body: OrganizationPlanOrderPaymentRequest }) => {
      const { data } = await submitOrganizationPlanOrderPayment({
        body,
        client: apiClient,
        path: { organization_id: organizationId, order_id: orderId },
      })
      return data
    },
    onSuccess: (data) => queryClient.setQueryData(
      organizationQueryKey(organizationId, 'plan-orders', data.order.order_id),
      data.order,
    ),
  })
}

/** 提取企业授权订单接口的稳定公开错误码。 */
export function organizationPlanOrderErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}

/** 生成企业授权订单可跨网络重试复用的 128 位幂等键。 */
export function createOrganizationPlanOrderIdempotencyKey() {
  const bytes = new Uint8Array(new ArrayBuffer(16))
  globalThis.crypto.getRandomValues(bytes)
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
}

/** 管理员按稳定游标读取全部企业套餐，历史停用快照也保留在列表中。 */
export function useAdminOrganizationPlans(pageSize = 25) {
  return useInfiniteQuery({
    queryKey: [...adminOrganizationPlansQueryKey, { pageSize }],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminOrganizationPlans({
        client: apiClient,
        query: { before: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 创建成功后失效管理列表和用户目录，始终使用服务端返回的价格快照。 */
export function useCreateAdminOrganizationPlan() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationPlanCreateRequest) => {
      const { data } = await createAdminOrganizationPlan({ client: apiClient, body })
      return data
    },
    onSuccess: () => Promise.all([
      queryClient.invalidateQueries({ queryKey: adminOrganizationPlansQueryKey }),
      queryClient.invalidateQueries({ queryKey: organizationPlanCatalogQueryKey }),
    ]),
  })
}

/** 以列表中读取的版本执行停用 CAS，不允许覆盖已变化的套餐事实。 */
export function useDisableAdminOrganizationPlan() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ planId, body }: { planId: string; body: OrganizationPlanDisableRequest }) => {
      const { data } = await disableAdminOrganizationPlan({
        client: apiClient,
        path: { plan_id: planId },
        body,
      })
      return data
    },
    onSuccess: () => Promise.all([
      queryClient.invalidateQueries({ queryKey: adminOrganizationPlansQueryKey }),
      queryClient.invalidateQueries({ queryKey: organizationPlanCatalogQueryKey }),
    ]),
  })
}
