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

// Header 值必须是浏览器可安全发送的 ASCII；具体操作与目标仍由服务端审计字段记录。
export const platformAdministratorReason = 'platform administrator console operation'

export { isValidPlatformOrganizationReason } from './platform-organization-model'

/** 进入管理员企业目录后直接读取，服务端继续执行平台权限校验与审计。 */
export function usePlatformOrganizations(enabled = true, pageSize = 25) {
  return useInfiniteQuery({
    queryKey: [...platformOrganizationQueryKey, 'directory', pageSize],
    enabled,
    gcTime: 0,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listPlatformOrganizations({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': platformAdministratorReason },
        query: { after: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}

/** 管理员打开目标用户后直接读取企业归属。 */
export function usePlatformUserOrganizations(userId: number | undefined, pageSize = 25) {
  return useInfiniteQuery({
    queryKey: [...platformOrganizationQueryKey, 'user', userId, pageSize],
    enabled: userId !== undefined,
    gcTime: 0,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (userId === undefined) throw new Error('缺少目标用户')
      const { data } = await listPlatformUserOrganizations({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': platformAdministratorReason },
        path: { user_id: userId },
        query: { after: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}

/** 管理员选择企业后直接读取成员身份。 */
export function usePlatformOrganizationMembers(organizationId: number | undefined, pageSize = 50) {
  return useInfiniteQuery({
    queryKey: [...platformOrganizationQueryKey, 'members', organizationId, pageSize],
    enabled: organizationId !== undefined,
    gcTime: 0,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (organizationId === undefined) throw new Error('缺少目标企业')
      const { data } = await listPlatformOrganizationMembers({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': platformAdministratorReason },
        path: { organization_id: organizationId },
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
export function useChangePlatformOrganizationMember(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ membershipId, body }: { membershipId: number; body: PlatformOrganizationMemberChangeRequest }) => {
      const { data } = await changePlatformOrganizationMember({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': platformAdministratorReason },
        path: { organization_id: organizationId, membership_id: membershipId },
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
export function useInvitePlatformOrganizationMember(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: PlatformOrganizationInvitationRequest) => {
      const { data } = await invitePlatformOrganizationMember({
        client: apiClient,
        headers: { 'x-anyflows-platform-reason': platformAdministratorReason },
        path: { organization_id: organizationId },
        body,
      })
      return data
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: platformOrganizationQueryKey })
    },
  })
}
