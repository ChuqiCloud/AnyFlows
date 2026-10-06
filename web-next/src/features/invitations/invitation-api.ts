import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { getUserInvitations } from '@/lib/api/generated/sdk.gen'

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
