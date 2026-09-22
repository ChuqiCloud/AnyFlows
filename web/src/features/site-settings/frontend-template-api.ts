import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  activateAdminFrontendTemplate,
  listAdminFrontendTemplates,
  scanAdminFrontendTemplates,
} from '@/lib/api/generated/sdk.gen'
import type {
  FrontendTemplateCatalog,
  FrontendTemplateSummary,
} from '@/lib/api/generated/types.gen'

export type { FrontendTemplateCatalog, FrontendTemplateSummary }

const bearerSecurity = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const
export const frontendTemplateCatalogQueryKey = ['admin-frontend-templates'] as const

/** 读取当前外部模板目录的扫描快照。 */
export function useFrontendTemplates() {
  return useQuery({
    queryKey: frontendTemplateCatalogQueryKey,
    staleTime: 60_000,
    refetchOnWindowFocus: false,
    queryFn: async ({ signal }) => {
      const { data } = await listAdminFrontendTemplates({
        client: apiClient,
        security: bearerSecurity,
        signal,
      })
      return data
    },
  })
}

/** 重新扫描运行目录中的模板，并替换列表快照。 */
export function useScanFrontendTemplates() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async () => {
      const { data } = await scanAdminFrontendTemplates({
        client: apiClient,
        security: bearerSecurity,
      })
      return data
    },
    onSuccess: (catalog) => {
      queryClient.setQueryData(frontendTemplateCatalogQueryKey, catalog)
      void queryClient.invalidateQueries({ queryKey: ['frontend-template-preview'] })
    },
  })
}

/** 原子切换当前模板；null 表示恢复内嵌前端。 */
export function useActivateFrontendTemplate() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (templateId: string | null) => {
      const { data } = await activateAdminFrontendTemplate({
        body: { template_id: templateId },
        client: apiClient,
        security: bearerSecurity,
      })
      return data
    },
    onSuccess: (catalog) => {
      queryClient.setQueryData(frontendTemplateCatalogQueryKey, catalog)
    },
  })
}
