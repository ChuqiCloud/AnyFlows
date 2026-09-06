import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  changePlatformOrganizationMember,
  invitePlatformOrganizationMember,
  listPlatformOrganizationMembers,
  listPlatformOrganizations,
  listPlatformUserOrganizations,
} from '@/lib/api/generated/sdk.gen'
import type {
  PlatformOrganizationInvitationRequest,
  PlatformOrganizationMemberChangeRequest,
} from '@/lib/api/generated/types.gen'

const platformOrganizationQueryKey = ['platform-organizations'] as const

export type PlatformOrganizationAccess = {
  reason: string
}

export type PlatformUserOrganizationAccess = PlatformOrganizationAccess & {
  userId: number
}

export type PlatformOrganizationMemberAccess = PlatformOrganizationAccess & {
  organizationId: number
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

/** 企业成员身份仅在平台管理员明确选择企业并提交理由后读取。 */
export function usePlatformOrganizationMembers(
  access: PlatformOrganizationMemberAccess | undefined,
  pageSize = 50,
) {
  return useInfiniteQuery({
    queryKey: [...platformOrganizationQueryKey, 'members', access?.organizationId, access?.reason, pageSize],
    enabled: access !== undefined,
    gcTime: 0,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (access === undefined) throw new Error('缺少平台企业成员访问参数')
      const { data } = await listPlatformOrganizationMembers({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': access.reason },
        path: { organization_id: access.organizationId },
        query: { after: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}

/** 平台成员变更保留服务端当前团队归属，并以成员版本执行 CAS。 */
export function useChangePlatformOrganizationMember(access: PlatformOrganizationMemberAccess) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ membershipId, body }: { membershipId: number; body: PlatformOrganizationMemberChangeRequest }) => {
      const { data } = await changePlatformOrganizationMember({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': access.reason },
        path: { organization_id: access.organizationId, membership_id: membershipId },
        body,
      })
      return data
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: platformOrganizationQueryKey })
    },
  })
}

/** 平台邀请只创建待接受事实，不会直接把目标用户写成企业成员。 */
export function useInvitePlatformOrganizationMember(access: PlatformOrganizationMemberAccess) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: PlatformOrganizationInvitationRequest) => {
      const { data } = await invitePlatformOrganizationMember({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': access.reason },
        path: { organization_id: access.organizationId },
        body,
      })
      return data
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: platformOrganizationQueryKey })
    },
  })
}
