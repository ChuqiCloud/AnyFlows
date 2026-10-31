import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { userTokenGroupsQueryKey } from '@/features/api-keys/api-key-group-api'
import { apiClient } from '@/lib/api'
import { getAdminGroupKeyPolicy, updateAdminGroupKeyPolicy } from '@/lib/api/generated/sdk.gen'
import type { AdminGroupKeyPolicy, AdminGroupKeyPolicyUpdateResult, AdminGroupKeyPolicyWriteRequest } from '@/lib/api/generated/types.gen'

export type GroupKeySelectionMode = 'default_only' | 'all_users' | 'selected_groups'
export type GroupKeyPolicy = Omit<AdminGroupKeyPolicy, 'selection_mode' | 'allowed_user_ids'> & {
  selection_mode: GroupKeySelectionMode
  allowed_group_ids: number[]
}
export type GroupKeyPolicyWriteRequest = {
  selection_mode: GroupKeySelectionMode
  allowed_group_ids: number[]
  disable_keys_losing_access?: boolean
}
export type GroupKeyPolicyUpdateResult = Omit<AdminGroupKeyPolicyUpdateResult, 'selection_mode' | 'allowed_user_ids'> & {
  selection_mode: GroupKeySelectionMode
  allowed_group_ids: number[]
}

const groupKeyPolicyQueryKey = ['admin-group-key-policy'] as const

export function useAdminGroupKeyPolicy(groupId: number) {
  return useQuery({
    queryKey: [...groupKeyPolicyQueryKey, groupId],
    refetchOnWindowFocus: false,
    queryFn: async ({ signal }): Promise<GroupKeyPolicy> => {
      const { data } = await getAdminGroupKeyPolicy({ client: apiClient, path: { id: groupId }, signal })
      return data as unknown as GroupKeyPolicy
    },
  })
}

export function useUpdateAdminGroupKeyPolicy(groupId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: GroupKeyPolicyWriteRequest): Promise<GroupKeyPolicyUpdateResult> => {
      const { data } = await updateAdminGroupKeyPolicy({ client: apiClient, path: { id: groupId }, body: body as unknown as AdminGroupKeyPolicyWriteRequest })
      return data as unknown as GroupKeyPolicyUpdateResult
    },
    onSuccess: (policy) => {
      queryClient.setQueryData([...groupKeyPolicyQueryKey, groupId], policy)
      void queryClient.invalidateQueries({ queryKey: userTokenGroupsQueryKey })
    },
  })
}
