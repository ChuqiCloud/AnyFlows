import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  createAdminOrganizationPlan,
  disableAdminOrganizationPlan,
  getOrganizationPlanCatalog,
  listAdminOrganizationPlans,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationPlanCreateRequest,
  OrganizationPlanDisableRequest,
} from '@/lib/api/generated/types.gen'

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
