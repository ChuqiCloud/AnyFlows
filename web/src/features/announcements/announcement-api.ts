import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  createAdminAnnouncement,
  listAdminAnnouncements,
  listPublicAnnouncements,
  publishAdminAnnouncement,
  revokeAdminAnnouncement,
  updateAdminAnnouncement,
} from '@/lib/api/generated/sdk.gen'
import type {
  AnnouncementMutationRequest,
  AnnouncementUpdateRequest,
  AnnouncementWriteRequest,
} from '@/lib/api/generated/types.gen'

export const publicAnnouncementsQueryKey = ['public-announcements'] as const
export const adminAnnouncementsQueryKey = ['admin-announcements'] as const

/** 读取游客可见公告；服务端已经过滤草稿、撤回和时间窗。 */
export function usePublicAnnouncements() {
  return useQuery({
    queryKey: publicAnnouncementsQueryKey,
    staleTime: 30_000,
    retry: false,
    queryFn: async ({ signal }) => {
      const { data } = await listPublicAnnouncements({ client: apiClient, signal })
      return data
    },
  })
}

/** 读取管理员公告全量版本事实。 */
export function useAdminAnnouncements() {
  return useQuery({
    queryKey: adminAnnouncementsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await listAdminAnnouncements({ client: apiClient, signal })
      return data
    },
  })
}

function invalidateAnnouncements(queryClient: ReturnType<typeof useQueryClient>) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: adminAnnouncementsQueryKey }),
    queryClient.invalidateQueries({ queryKey: publicAnnouncementsQueryKey }),
  ])
}

/** 创建公告草稿。 */
export function useCreateAdminAnnouncement() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AnnouncementWriteRequest) => {
      const { data } = await createAdminAnnouncement({ body, client: apiClient })
      return data
    },
    onSuccess: () => invalidateAnnouncements(queryClient),
  })
}

/** 按版本更新草稿，冲突由页面保留当前输入并要求刷新。 */
export function useUpdateAdminAnnouncement() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AnnouncementUpdateRequest }) => {
      const { data } = await updateAdminAnnouncement({ path: { id }, body, client: apiClient })
      return data
    },
    onSuccess: () => invalidateAnnouncements(queryClient),
  })
}

function useAnnouncementTransition(action: typeof publishAdminAnnouncement) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AnnouncementMutationRequest }) => {
      const { data } = await action({ path: { id }, body, client: apiClient })
      return data
    },
    onSuccess: () => invalidateAnnouncements(queryClient),
  })
}

/** 发布草稿。 */
export function usePublishAdminAnnouncement() {
  return useAnnouncementTransition(publishAdminAnnouncement)
}

/** 撤回已发布公告。 */
export function useRevokeAdminAnnouncement() {
  return useAnnouncementTransition(revokeAdminAnnouncement)
}
