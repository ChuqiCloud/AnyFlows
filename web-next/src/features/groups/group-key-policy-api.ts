import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { userTokenGroupsQueryKey } from '@/features/api-keys/api-key-group-api'
import { apiClient } from '@/lib/api'
import { getAdminGroupKeyPolicy, updateAdminGroupKeyPolicy } from '@/lib/api/generated/sdk.gen'

export type GroupKeySelectionMode = 'default_only' | 'all_users' | 'selected_users'
export type GroupKeyPolicy = {
  group_id: number
  selection_mode: GroupKeySelectionMode
  allowed_user_ids: number[]
}
export type GroupKeyPolicyWriteRequest = Omit<GroupKeyPolicy, 'group_id'>
export type GroupKeyPolicyUpdateResult = GroupKeyPolicy & { disabled_key_count: number; disabled_user_count: number }

const groupKeyPolicyQueryKey = ['admin-group-key-policy'] as const

export function useAdminGroupKeyPolicy(groupId: number) {
  return useQuery({
    queryKey: [...groupKeyPolicyQueryKey, groupId],
    refetchOnWindowFocus: false,
    queryFn: async ({ signal }): Promise<GroupKeyPolicy> => {
      const { data } = await getAdminGroupKeyPolicy({ client: apiClient, path: { id: groupId }, signal })
      return data
    },
  })
}

export function useUpdateAdminGroupKeyPolicy(groupId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: GroupKeyPolicyWriteRequest & { disable_keys_losing_access?: boolean }): Promise<GroupKeyPolicyUpdateResult> => {
      const { data } = await updateAdminGroupKeyPolicy({ client: apiClient, path: { id: groupId }, body })
      return data
    },
    onSuccess: (policy) => {
      queryClient.setQueryData([...groupKeyPolicyQueryKey, groupId], policy)
      void queryClient.invalidateQueries({ queryKey: userTokenGroupsQueryKey })
    },
  })
}
