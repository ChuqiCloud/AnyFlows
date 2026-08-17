import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  changeUserPassword,
  disableUserTwoFactor,
  enableUserTwoFactor,
  getUserProfile,
  getUserTwoFactor,
  updateUserNotificationPreferences,
  updateUserProfile,
} from '@/lib/api/generated/sdk.gen'
import type {
  UserNotificationPreferencesRequest,
  UserPasswordChangeRequest,
  UserProfileResponse,
  UserProfileUpdateRequest,
  UserTwoFactorPasswordRequest,
} from '@/lib/api/generated/types.gen'

export const userProfileQueryKey = ['user-profile'] as const
export const userTwoFactorQueryKey = ['user-two-factor'] as const

/** 读取当前会话用户的个人资料与通知偏好。 */
export function useUserProfile() {
  return useQuery({
    queryKey: userProfileQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getUserProfile({ client: apiClient, signal })
      return data
    },
  })
}

/** 更新用户名后立即刷新当前资料缓存。 */
export function useUpdateUserProfile() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: UserProfileUpdateRequest) => {
      const { data } = await updateUserProfile({ body, client: apiClient })
      return data
    },
    onSuccess: (profile) => queryClient.setQueryData(userProfileQueryKey, profile),
  })
}

/** 修改密码；服务端成功后会撤销所有旧会话。 */
export function useChangeUserPassword() {
  return useMutation({
    mutationFn: async (body: UserPasswordChangeRequest) => {
      await changeUserPassword({ body, client: apiClient })
    },
  })
}

/** 读取当前用户的 TOTP 开关状态；响应不包含任何密钥材料。 */
export function useUserTwoFactor() {
  return useQuery({
    queryKey: userTwoFactorQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getUserTwoFactor({ client: apiClient, signal })
      return data
    },
  })
}

/** 校验当前密码并启用 TOTP，入网材料只由本次 mutation 返回。 */
export function useEnableUserTwoFactor() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: UserTwoFactorPasswordRequest) => {
      const { data } = await enableUserTwoFactor({ body, client: apiClient })
      return data
    },
    onSuccess: (status) => queryClient.setQueryData(userTwoFactorQueryKey, { enabled: status.enabled }),
  })
}

/** 校验当前密码并停用 TOTP，同时清除旧的状态缓存。 */
export function useDisableUserTwoFactor() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: UserTwoFactorPasswordRequest) => {
      await disableUserTwoFactor({ body, client: apiClient })
    },
    onSuccess: () => queryClient.setQueryData(userTwoFactorQueryKey, { enabled: false }),
  })
}

/** 保存邮件通知偏好并同步资料缓存。 */
export function useUpdateUserNotificationPreferences() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: UserNotificationPreferencesRequest) => {
      const { data } = await updateUserNotificationPreferences({ body, client: apiClient })
      return data
    },
    onSuccess: (profile) => queryClient.setQueryData<UserProfileResponse>(userProfileQueryKey, profile),
  })
}
