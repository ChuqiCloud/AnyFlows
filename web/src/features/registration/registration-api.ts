import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  getAdminRegistrationPolicy,
  getRegistrationStatus,
  registerUser,
  updateAdminRegistrationPolicy,
} from '@/lib/api/generated/sdk.gen'
import type {
  RegistrationPolicyRequest,
  RegistrationRequestWritable,
} from '@/lib/api/generated/types.gen'

export const registrationStatusQueryKey = ['registration-status'] as const
export const adminRegistrationPolicyQueryKey = ['admin-registration-policy'] as const

/** 读取游客可见的最小注册状态，任何失败都由页面按关闭能力处理。 */
export function useRegistrationStatus() {
  return useQuery({
    queryKey: registrationStatusQueryKey,
    staleTime: 30_000,
    retry: false,
    queryFn: async ({ signal }) => {
      const { data } = await getRegistrationStatus({ client: apiClient, signal })
      return data
    },
  })
}

/** 提交公开注册，成功响应直接复用统一登录会话契约。 */
export function useRegisterUser() {
  return useMutation({
    mutationFn: async (body: RegistrationRequestWritable) => {
      const { data } = await registerUser({ body, client: apiClient })
      return data
    },
  })
}

/** 读取管理员完整注册策略。 */
export function useAdminRegistrationPolicy() {
  return useQuery({
    queryKey: adminRegistrationPolicyQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminRegistrationPolicy({ client: apiClient, signal })
      return data
    },
  })
}

/** 覆盖完整策略，并同步管理员与公开状态缓存。 */
export function useUpdateAdminRegistrationPolicy() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: RegistrationPolicyRequest) => {
      const { data } = await updateAdminRegistrationPolicy({ body, client: apiClient })
      return data
    },
    onSuccess: (policy) => {
      queryClient.setQueryData(adminRegistrationPolicyQueryKey, policy)
      queryClient.setQueryData(registrationStatusQueryKey, {
        enabled: policy.enabled,
        email_required: policy.email_required,
      })
    },
  })
}
