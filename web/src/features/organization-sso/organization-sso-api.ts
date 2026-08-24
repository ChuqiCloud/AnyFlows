import { useQuery } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient } from '@/lib/api'
import { listOrganizationSsoProviders } from '@/lib/api/generated/sdk.gen'

/** 按企业权限读取脱敏 SSO Provider；未授权时不发起请求。 */
export function useOrganizationSsoProviders(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'sso', 'providers'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationSsoProviders({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data.providers
    },
  })
}
