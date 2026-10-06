import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { getInitialSetupStatus } from '@/lib/api/generated/sdk.gen'
import type { SetupStatusResponse } from '@/lib/api/generated/types.gen'
import { appQueryClient } from '@/lib/query/query-client'

export const initialSetupStatusQueryKey = ['initial-setup-status'] as const

/** 读取失败关闭的首次安装状态，避免服务异常时错误放行登录入口。 */
export function useInitialSetupStatus() {
  return useQuery({
    queryFn: async () => {
      const { data } = await getInitialSetupStatus({ client: apiClient })
      return data
    },
    queryKey: initialSetupStatusQueryKey,
    staleTime: Number.POSITIVE_INFINITY,
  })
}

/** 安装成功后同步关闭当前标签页中的 setup 入口。 */
export function markInitialSetupComplete() {
  appQueryClient.setQueryData<SetupStatusResponse>(initialSetupStatusQueryKey, {
    setup_required: false,
  })
}
