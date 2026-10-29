import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { getAdminGroupModelPolicy, updateAdminGroupModelPolicy } from '@/lib/api/generated/sdk.gen'
import type { AdminGroupModelPolicy, AdminGroupModelPolicyWriteRequest } from '@/lib/api/generated/types.gen'

export type GroupModelPolicy = AdminGroupModelPolicy
export type GroupModelPolicyWriteRequest = AdminGroupModelPolicyWriteRequest

const groupModelPolicyQueryKey = ['admin-group-model-policy'] as const

async function readPolicy(groupId: number, signal?: AbortSignal): Promise<GroupModelPolicy> {
  const { data } = await getAdminGroupModelPolicy({
    client: apiClient,
    path: { id: groupId },
    signal,
  })
  return data
}

export function useAdminGroupModelPolicy(groupId: number) {
  return useQuery({
    queryKey: [...groupModelPolicyQueryKey, groupId],
    refetchOnWindowFocus: false,
    queryFn: ({ signal }) => readPolicy(groupId, signal),
  })
}

export function useUpdateAdminGroupModelPolicy(groupId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: GroupModelPolicyWriteRequest) => {
      const { data } = await updateAdminGroupModelPolicy({
        client: apiClient,
        path: { id: groupId },
        body,
      })
      return data
    },
    onSuccess: (policy) => {
      queryClient.setQueryData([...groupModelPolicyQueryKey, groupId], policy)
      void queryClient.invalidateQueries({ queryKey: ['model-catalog'] })
      void queryClient.invalidateQueries({ queryKey: ['model-catalog-providers'] })
    },
  })
}
