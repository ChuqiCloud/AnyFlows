import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { listAdminGroups } from '@/lib/api/generated/sdk.gen'
import type { AdminGroup } from '@/lib/api/generated/types.gen'

export const adminGroupCatalogQueryKey = ['admin-group-catalog'] as const

/** 自动遍历稳定游标，为管理表单提供完整且一致的分组目录。 */
export function useAdminGroupCatalog() {
  return useQuery({
    queryKey: adminGroupCatalogQueryKey,
    staleTime: 60_000,
    queryFn: async ({ signal }) => {
      const groups: AdminGroup[] = []
      const seenCursors = new Set<number>()
      let after: number | undefined
      while (true) {
        const { data } = await listAdminGroups({
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
