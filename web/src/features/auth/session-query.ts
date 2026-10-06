import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { getManagementSession } from '@/lib/api/generated/sdk.gen'
import type { LoginResponse, SessionResponse } from '@/lib/api/generated/types.gen'
import {
  clearManagementSessionToken,
  setManagementSessionToken,
} from '@/lib/api/session-token'
import { appQueryClient } from '@/lib/query/query-client'

export const managementSessionQueryKey = ['management-session'] as const

/** 查询并验证服务端管理会话，失败状态由鉴权边界统一呈现。 */
export function useManagementSession(enabled: boolean) {
  return useQuery({
    enabled,
    queryFn: async () => {
      const { data } = await getManagementSession({ client: apiClient })
      return data
    },
    queryKey: managementSessionQueryKey,
  })
}

/** 登录成功后原子写入令牌和对应会话快照，避免控制台重复请求。 */
export function establishManagementSession(response: LoginResponse) {
  setManagementSessionToken(response.access_token)
  appQueryClient.setQueryData<SessionResponse>(managementSessionQueryKey, {
    expires_at: response.expires_at,
    user: response.user,
  })
}

/** 主动结束当前管理会话，并清理所有受保护查询的内存缓存。 */
export function endManagementSession() {
  clearManagementSessionToken()
  appQueryClient.clear()
}
