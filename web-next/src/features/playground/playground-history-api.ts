import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  deletePlaygroundConversation,
  getPlaygroundConversation,
  listPlaygroundConversations,
  savePlaygroundConversation as savePlaygroundConversationRequest,
} from '@/lib/api/generated/sdk.gen'
import type {
  PlaygroundConversationResponse,
  PlaygroundConversationSummary,
  PlaygroundShareSession,
} from '@/lib/api/generated/types.gen'

export type { PlaygroundConversationResponse, PlaygroundConversationSummary }

export const playgroundHistoryQueryKey = ['playground-conversations'] as const

/** 保存一份已裁剪为可见完整往返的私有会话快照。 */
export async function savePlaygroundConversation(input: {
  conversationId: string
  revision?: number
  sessions: PlaygroundShareSession[]
}) {
  const { data } = await savePlaygroundConversationRequest({
    body: { revision: input.revision ?? null, sessions: input.sessions },
    client: apiClient,
    path: { conversation_id: input.conversationId },
  })
  return data
}

export function usePlaygroundHistoryList(enabled: boolean) {
  return useQuery({
    queryKey: playgroundHistoryQueryKey,
    enabled,
    staleTime: 0,
    queryFn: async ({ signal }) => {
      const { data } = await listPlaygroundConversations({
        client: apiClient,
        signal,
      })
      return data
    },
  })
}

export function useReadPlaygroundConversation() {
  return useMutation({
    mutationFn: async (conversationId: string) => {
      const { data } = await getPlaygroundConversation({
        client: apiClient,
        path: { conversation_id: conversationId },
      })
      return data
    },
  })
}

export function useDeletePlaygroundConversation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (conversationId: string) => {
      await deletePlaygroundConversation({
        client: apiClient,
        path: { conversation_id: conversationId },
      })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: playgroundHistoryQueryKey }),
  })
}
