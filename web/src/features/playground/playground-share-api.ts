import { useMutation, useQuery } from '@tanstack/react-query'
import { useRef } from 'react'

import { apiClient } from '@/lib/api'
import {
  createPlaygroundShare,
  getPlaygroundShare,
  revokePlaygroundShare,
} from '@/lib/api/generated/sdk.gen'
import type { PlaygroundShareSession } from '@/lib/api/generated/types.gen'

let publicShareQuerySequence = 0

function nextPublicShareQueryIdentity() {
  publicShareQuerySequence += 1
  return publicShareQuerySequence
}

export function useCreatePlaygroundShare() {
  return useMutation({
    mutationFn: async ({
      sessions,
      ttlDays,
    }: {
      sessions: PlaygroundShareSession[]
      ttlDays: 1 | 7 | 30
    }) => {
      const { data } = await createPlaygroundShare({
        body: { sessions, ttl_days: ttlDays },
        client: apiClient,
      })
      return data
    },
  })
}

export function useRevokePlaygroundShare() {
  return useMutation({
    mutationFn: async (token: string) => {
      await revokePlaygroundShare({ client: apiClient, path: { token } })
    },
  })
}

/** 公开读取不把明文令牌放入 Query key，并在离开页面后立即释放快照缓存。 */
export function usePublicPlaygroundShare(token?: string) {
  const queryIdentity = useRef<{ token?: string; value: number } | undefined>(undefined)
  if (!queryIdentity.current || queryIdentity.current.token !== token) {
    queryIdentity.current = { token, value: nextPublicShareQueryIdentity() }
  }

  return useQuery({
    queryKey: ['public-playground-share', queryIdentity.current.value],
    enabled: token !== undefined,
    queryFn: async ({ signal }) => {
      if (!token) throw new Error('分享令牌缺失')
      const { data } = await getPlaygroundShare({
        client: apiClient,
        path: { token },
        signal,
      })
      return data
    },
    gcTime: 0,
    retry: false,
    staleTime: 0,
    refetchOnWindowFocus: false,
  })
}
