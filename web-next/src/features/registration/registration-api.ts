import { useMutation } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  registerUser,
  sendRegistrationEmailVerification,
} from '@/lib/api/generated/sdk.gen'
import type {
  RegistrationEmailVerificationRequest,
  RegistrationRequestWritable,
} from '@/lib/api/generated/types.gen'

/** 提交公开注册，成功响应直接复用统一登录会话契约。 */
export function useRegisterUser() {
  return useMutation({
    mutationFn: async (body: RegistrationRequestWritable) => {
      const { data } = await registerUser({ body, client: apiClient })
      return data
    },
  })
}

/** 请求服务端发送注册邮箱验证码。 */
export function useSendRegistrationEmailVerification() {
  return useMutation({
    mutationFn: async (body: RegistrationEmailVerificationRequest) => {
      const { data } = await sendRegistrationEmailVerification({ body, client: apiClient })
      return data
    },
  })
}
