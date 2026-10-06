import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import { adminChannelsQueryKey } from '@/features/channels/channel-api'
import {
  applyAdminModelSyncPreview,
  createAdminModelSyncPreview,
  importMissingAdminModels,
  listMissingAdminModels,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminMissingModelImportRequest,
  AdminModelSyncApplyRequest,
} from '@/lib/api/generated/types.gen'
import { adminModelMetadataQueryKey } from './model-management-api'

export const adminMissingModelsQueryKey = ['admin-model-metadata-missing'] as const

/** 按 Canonical 稳定游标读取仍被活动渠道引用的缺失元数据。 */
export function useMissingAdminModels() {
  return useInfiniteQuery({
    queryKey: adminMissingModelsQueryKey,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listMissingAdminModels({
        client: apiClient,
        query: { after: pageParam, limit: 50 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useCreateModelSyncPreview() {
  return useMutation({
    mutationFn: async (channelId: number) => {
      const { data } = await createAdminModelSyncPreview({
        body: { channel_id: channelId },
        client: apiClient,
      })
      return data
    },
  })
}

export function useApplyModelSyncPreview() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ previewId, body }: {
      previewId: string
      body: AdminModelSyncApplyRequest
    }) => {
      const { data } = await applyAdminModelSyncPreview({
        body,
        client: apiClient,
        path: { preview_id: previewId },
      })
      return data
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: adminMissingModelsQueryKey }),
        queryClient.invalidateQueries({ queryKey: adminModelMetadataQueryKey }),
        queryClient.invalidateQueries({ queryKey: ['model-catalog'] }),
        queryClient.invalidateQueries({ queryKey: adminChannelsQueryKey }),
        queryClient.invalidateQueries({ queryKey: ['admin-channel'] }),
      ])
    },
  })
}

export function useImportMissingAdminModels() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminMissingModelImportRequest) => {
      const { data } = await importMissingAdminModels({ body, client: apiClient })
      return data
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: adminMissingModelsQueryKey }),
        queryClient.invalidateQueries({ queryKey: adminModelMetadataQueryKey }),
        queryClient.invalidateQueries({ queryKey: ['model-catalog'] }),
      ])
    },
  })
}

/** 提取服务端闭合错误码，界面只据此选择可操作文案。 */
export function modelSyncErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  const details = error.details as { code?: unknown }
  return typeof details.code === 'string' ? details.code : undefined
}
