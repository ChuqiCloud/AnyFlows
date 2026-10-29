import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { jsonBodySerializer } from '@/lib/api/generated/client'

export type GroupModelPolicy = {
  group_id: number
  allowed_models: string[] | null
}

export type GroupModelPolicyWriteRequest = {
  allowed_models: string[] | null
}

const groupModelPolicyQueryKey = ['admin-group-model-policy'] as const
const managementSessionSecurity = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

async function readPolicy(groupId: number, signal?: AbortSignal): Promise<GroupModelPolicy> {
  const response = await apiClient.get({
    security: managementSessionSecurity,
    url: `/api/admin/groups/${groupId}/model-policy`,
    signal,
  }) as unknown as { data: GroupModelPolicy }
  return response.data
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
      const response = await apiClient.put({
        security: managementSessionSecurity,
        url: `/api/admin/groups/${groupId}/model-policy`,
        body,
        bodySerializer: jsonBodySerializer.bodySerializer,
      }) as unknown as { data: GroupModelPolicy }
      return response.data
    },
    onSuccess: (policy) => {
      queryClient.setQueryData([...groupModelPolicyQueryKey, groupId], policy)
      void queryClient.invalidateQueries({ queryKey: ['model-catalog'] })
      void queryClient.invalidateQueries({ queryKey: ['model-catalog-providers'] })
    },
  })
}
