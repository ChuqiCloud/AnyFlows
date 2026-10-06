import { useMutation } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { confirmPasswordReset, requestPasswordReset } from '@/lib/api/generated/sdk.gen'
import type {
  PasswordResetConfirmRequest,
  PasswordResetRequest,
} from '@/lib/api/generated/types.gen'

/** 请求密码重置邮件；服务端统一返回 accepted，不在客户端区分邮箱是否存在。 */
export function useRequestPasswordReset() {
  return useMutation({
    mutationFn: async (body: PasswordResetRequest) => {
      const { data } = await requestPasswordReset({ body, client: apiClient })
      return data
    },
  })
}

/** 消费单次重置令牌并提交新密码；令牌不进入 Query 缓存或持久化存储。 */
export function useConfirmPasswordReset() {
  return useMutation({
    mutationFn: async (body: PasswordResetConfirmRequest) => {
      const { data } = await confirmPasswordReset({ body, client: apiClient })
      return data
    },
  })
}
