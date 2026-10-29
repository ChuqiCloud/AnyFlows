import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { userTokenGroupsQueryKey } from '@/features/api-keys/api-key-group-api'
import { apiClient } from '@/lib/api'
import { getAdminGroupKeyPolicy, updateAdminGroupKeyPolicy } from '@/lib/api/generated/sdk.gen'
import type { AdminGroupKeyPolicy, AdminGroupKeyPolicyUpdateResult, AdminGroupKeyPolicyWriteRequest, GroupKeySelectionMode as ApiGroupKeySelectionMode } from '@/lib/api/generated/types.gen'

export type GroupKeySelectionMode = ApiGroupKeySelectionMode
export type GroupKeyPolicy = AdminGroupKeyPolicy
export type GroupKeyPolicyWriteRequest = AdminGroupKeyPolicyWriteRequest
export type GroupKeyPolicyUpdateResult = AdminGroupKeyPolicyUpdateResult

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
    mutationFn: async (body: GroupKeyPolicyWriteRequest): Promise<GroupKeyPolicyUpdateResult> => {
      const { data } = await updateAdminGroupKeyPolicy({ client: apiClient, path: { id: groupId }, body })
      return data
    },
    onSuccess: (policy) => {
      queryClient.setQueryData([...groupKeyPolicyQueryKey, groupId], policy)
      void queryClient.invalidateQueries({ queryKey: userTokenGroupsQueryKey })
    },
  })
}
