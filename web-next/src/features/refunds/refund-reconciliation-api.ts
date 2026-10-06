import { useInfiniteQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  listAccountRefundReconciliations,
  listAdminRefundReconciliations,
  listOrganizationRefundReconciliations,
} from '@/lib/api/generated/sdk.gen'

export type RefundReconciliationScope = 'account' | 'organization' | 'admin'

/** 按资金主体读取退款成功对账事实，游标始终由服务端提供。 */
export function useRefundReconciliations(
  scope: RefundReconciliationScope,
  organizationId?: number,
  enabled = true,
) {
  return useInfiniteQuery({
    queryKey: ['refund-reconciliations', scope, organizationId],
    enabled: enabled && (scope !== 'organization' || organizationId !== undefined),
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (scope === 'organization') {
        if (organizationId === undefined) throw new Error('organization id is required')
        const { data } = await listOrganizationRefundReconciliations({
          client: apiClient,
          path: { organization_id: organizationId },
          query: { before: pageParam, limit: 25 },
          signal,
        })
        return data
      }
      if (scope === 'admin') {
        const { data } = await listAdminRefundReconciliations({
          client: apiClient,
          query: { before: pageParam, limit: 25 },
          signal,
        })
        return data
      }
      const { data } = await listAccountRefundReconciliations({
        client: apiClient,
        query: { before: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}
