import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { listUserTokenGroups } from '@/lib/api/generated/sdk.gen'
import type { SelectableTokenGroup } from './api-key-group-model'

export const userTokenGroupsQueryKey = ['user-token-groups'] as const

/** 只有全部页读取成功才公开可选目录，避免使用不完整的授权快照。 */
export function useUserTokenGroups(enabled = true) {
  return useQuery({
    queryKey: userTokenGroupsQueryKey,
    enabled,
    queryFn: async ({ signal }) => {
      const groups: SelectableTokenGroup[] = []
      const seenCursors = new Set<number>()
      let after: number | undefined
      while (true) {
        const { data } = await listUserTokenGroups({
          client: apiClient,
          query: { after, limit: 100 },
          signal,
        })
        groups.push(...data.groups)
        const nextCursor = data.next_cursor ?? undefined
        if (nextCursor === undefined) return groups
        if (seenCursors.has(nextCursor)) throw new Error('分组列表返回了重复游标')
        seenCursors.add(nextCursor)
        after = nextCursor
      }
    },
  })
}
