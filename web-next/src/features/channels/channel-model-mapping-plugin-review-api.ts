import { useMutation, useQuery } from '@tanstack/react-query'
import { apiClient } from '@/lib/api'

export type ChannelModelMappingPluginReviewer = {
  plugin_id: string
  display_name: string
  plugin_version: string
}

export type ChannelModelMappingPluginReviewCheck = {
  code: string
  title: string
  status: 'pass' | 'info' | 'warning' | 'fail'
  message: string
  recommendation: string | null
}

export type ChannelModelMappingPluginReview = {
  plugin_id: string
  display_name: string
  plugin_version: string
  model_count: number
  mapping_count: number
  summary: string
  checks: ChannelModelMappingPluginReviewCheck[]
}

const key = ['channel-model-mapping-plugin-review'] as const
const security = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

export function useChannelModelMappingPluginReviewers(enabled: boolean) {
  return useQuery({
    queryKey: [...key, 'reviewers'],
    enabled,
    staleTime: 60_000,
    queryFn: async ({ signal }) => (await apiClient.get<
      { 200: { plugins: ChannelModelMappingPluginReviewer[] } }, unknown, true
    >({ url: '/api/admin/channel-model-mapping/plugin-reviewers', security, signal })).data,
  })
}

export function useReviewChannelModelMappingWithPlugin() {
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ channelId, pluginId }: { channelId: number; pluginId: string }) => (await apiClient.post<
      { 200: ChannelModelMappingPluginReview }, unknown, true
    >({
      url: '/api/admin/channels/{channel_id}/model-mapping/plugin-review',
      path: { channel_id: channelId },
      body: { plugin_id: pluginId },
      headers: { 'Content-Type': 'application/json' },
      security,
    })).data,
  })
}
