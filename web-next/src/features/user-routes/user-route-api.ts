import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import type { UserRoute, UserRouteList, UserRouteWriteRequest } from './user-route-model'

export const userRoutesQueryKey = ['user-routes'] as const
const security = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

export function useUserRoutes(enabled = true) {
  return useQuery({
    queryKey: userRoutesQueryKey,
    enabled,
    queryFn: async ({ signal }) => (await apiClient.get<{ 200: UserRouteList }, unknown, true>({
      url: '/api/user-routes', security, signal, throwOnError: true,
    })).data,
  })
}

export function useSaveUserRoute() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id?: number; body: UserRouteWriteRequest }) => {
      if (id !== undefined) return (await apiClient.put<{ 200: UserRoute }, unknown, true>({
        url: `/api/user-routes/${id}`, body, headers: { 'Content-Type': 'application/json' }, security, throwOnError: true,
      })).data
      return (await apiClient.post<{ 201: UserRoute }, unknown, true>({
        url: '/api/user-routes', body, headers: { 'Content-Type': 'application/json' }, security, throwOnError: true,
      })).data
    },
    onSuccess: () => Promise.all([
      queryClient.invalidateQueries({ queryKey: userRoutesQueryKey }),
      queryClient.invalidateQueries({ queryKey: ['user-api-keys'] }),
    ]),
  })
}

export function useDeleteUserRoute() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      await apiClient.delete({ url: `/api/user-routes/${id}`, security, throwOnError: true })
    },
    onSuccess: () => Promise.all([
      queryClient.invalidateQueries({ queryKey: userRoutesQueryKey }),
      queryClient.invalidateQueries({ queryKey: ['user-api-keys'] }),
    ]),
  })
}

export function userRouteErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  return 'code' in error.details && typeof error.details.code === 'string' ? error.details.code : undefined
}
