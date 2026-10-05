import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  confirmUserEmailBinding,
  changeUserPassword,
  disableUserTwoFactor,
  enableUserTwoFactor,
  finishUserPasskeyRegistration,
  getUserProfile,
  getUserTwoFactor,
  listUserPasskeys,
  listUserNotifications,
  markUserNotificationsRead,
  renameUserPasskey,
  revokeUserPasskey,
  sendUserEmailBindingVerification,
  startUserPasskeyRegistration,
  updateUserNotificationPreferences,
  updateUserProfile,
} from '@/lib/api/generated/sdk.gen'
import type {
  UserEmailBindingConfirmRequestWritable,
  UserEmailBindingVerificationRequest,
  UserNotificationPreferencesRequest,
  UserPasswordChangeRequest,
  UserProfileResponse,
  UserProfileUpdateRequest,
  UserPasskeyRegistrationVerifyRequest,
  UserPasskeyRenameRequest,
  UserPasskeyRevokeRequest,
  UserTwoFactorPasswordRequest,
} from '@/lib/api/generated/types.gen'

export const userProfileQueryKey = ['user-profile'] as const
export const userTwoFactorQueryKey = ['user-two-factor'] as const
export const userPasskeysQueryKey = ['user-passkeys'] as const
export const userNotificationsQueryKey = ['user-notifications'] as const

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

/** 向待绑定邮箱发送验证码。 */
export function useSendUserEmailBindingVerification() {
  return useMutation({
    mutationFn: async (body: UserEmailBindingVerificationRequest) => {
      const { data } = await sendUserEmailBindingVerification({ body, client: apiClient })
      return data
    },
  })
}

/** 使用验证码绑定或更换当前用户邮箱，并刷新资料缓存。 */
export function useConfirmUserEmailBinding() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: UserEmailBindingConfirmRequestWritable) => {
      const { data } = await confirmUserEmailBinding({ body, client: apiClient })
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

/** 读取当前用户的脱敏 Passkey 目录。 */
export function useUserPasskeys() {
  return useQuery({
    queryKey: userPasskeysQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await listUserPasskeys({ client: apiClient, signal })
      return data
    },
  })
}

/** 创建一次性 WebAuthn 注册挑战。 */
export function useStartUserPasskeyRegistration() {
  return useMutation({
    mutationFn: async () => {
      const { data } = await startUserPasskeyRegistration({ client: apiClient })
      return data
    },
  })
}

/** 提交浏览器生成的 WebAuthn 注册响应并刷新目录。 */
export function useFinishUserPasskeyRegistration() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: UserPasskeyRegistrationVerifyRequest) => {
      const { data } = await finishUserPasskeyRegistration({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: userPasskeysQueryKey }),
  })
}

/** 修改 Passkey 展示名称并保持当前目录缓存一致。 */
export function useRenameUserPasskey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, ...body }: UserPasskeyRenameRequest & { id: number }) => {
      const { data } = await renameUserPasskey({ path: { id }, body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: userPasskeysQueryKey }),
  })
}

/** 通过密码和可选二次验证码撤销 Passkey。 */
export function useRevokeUserPasskey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, ...body }: UserPasskeyRevokeRequest & { id: number }) => {
      await revokeUserPasskey({ path: { id }, body, client: apiClient })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: userPasskeysQueryKey }),
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

/** 按稳定复合游标读取当前用户的通知事实历史。 */
export function useUserNotifications(enabled = true) {
  return useInfiniteQuery({
    enabled,
    queryKey: userNotificationsQueryKey,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listUserNotifications({
        client: apiClient,
        query: { before: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 将通知事实标记为已读，并让所有消费者刷新服务端维护的未读数。 */
export function useMarkUserNotificationsRead() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (notificationIds: number[]) => {
      const { data } = await markUserNotificationsRead({
        body: { notification_ids: notificationIds },
        client: apiClient,
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: userNotificationsQueryKey }),
  })
}
