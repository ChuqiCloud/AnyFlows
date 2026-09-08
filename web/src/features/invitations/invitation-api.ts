import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { organizationDirectoryQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient } from '@/lib/api'
import { acceptOrganizationInvitation, getUserInvitations } from '@/lib/api/generated/sdk.gen'

export const userInvitationsQueryKey = ['user-invitations'] as const

/** 读取当前登录用户自己的邀请汇总。 */
export function useUserInvitations() {
  return useQuery({
    queryKey: userInvitationsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getUserInvitations({ client: apiClient, signal })
      return data
    },
  })
}

/** 接受企业邀请后刷新工作空间目录，服务端仍按登录邮箱和令牌摘要复验。 */
export function useAcceptOrganizationInvitation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (token: string) => {
      const { data } = await acceptOrganizationInvitation({
        body: { token },
        client: apiClient,
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: organizationDirectoryQueryKey }),
  })
}
