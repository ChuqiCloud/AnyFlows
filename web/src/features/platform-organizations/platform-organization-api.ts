import { useInfiniteQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  listPlatformOrganizations,
  listPlatformUserOrganizations,
} from '@/lib/api/generated/sdk.gen'

const platformOrganizationQueryKey = ['platform-organizations'] as const

export type PlatformOrganizationAccess = {
  reason: string
}

export type PlatformUserOrganizationAccess = PlatformOrganizationAccess & {
  userId: number
}

export { isValidPlatformOrganizationReason } from './platform-organization-model'

/** 仅在管理员明确提交临时理由后读取跨企业目录。 */
export function usePlatformOrganizations(
  access: PlatformOrganizationAccess | undefined,
  pageSize = 25,
) {
  return useInfiniteQuery({
    queryKey: [...platformOrganizationQueryKey, 'directory', access?.reason, pageSize],
    enabled: access !== undefined,
    gcTime: 0,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (access === undefined) throw new Error('缺少平台企业目录访问理由')
      const { data } = await listPlatformOrganizations({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': access.reason },
        query: { after: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}

/** 用户归属只在管理员打开目标用户并提交临时理由后读取。 */
export function usePlatformUserOrganizations(
  access: PlatformUserOrganizationAccess | undefined,
  pageSize = 25,
) {
  return useInfiniteQuery({
    queryKey: [...platformOrganizationQueryKey, 'user', access?.userId, access?.reason, pageSize],
    enabled: access !== undefined,
    gcTime: 0,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (access === undefined) throw new Error('缺少用户企业归属访问参数')
      const { data } = await listPlatformUserOrganizations({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': access.reason },
        path: { user_id: access.userId },
        query: { after: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}
