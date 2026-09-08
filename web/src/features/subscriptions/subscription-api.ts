import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  bindAdminUserSubscription,
  createAdminSubscriptionPlan,
  createCurrentSubscriptionOrder,
  disableAdminSubscriptionPlan,
  listAdminSubscriptionPlans,
  listAdminUserSubscriptions,
  listCurrentSubscriptionCatalog,
  listCurrentUserSubscriptions,
  getCurrentSubscriptionOrder,
  submitCurrentSubscriptionOrderPayment,
  transitionAdminUserSubscriptionLifecycle,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminSubscriptionPlanCreateRequest,
  AdminSubscriptionPlanDisableRequest,
  AdminUserSubscriptionBindRequest,
  AdminUserSubscriptionLifecycleRequest,
  SubscriptionOrderCreateRequest,
  SubscriptionOrderPaymentRequest,
} from '@/lib/api/generated/types.gen'

export const adminSubscriptionPlansQueryKey = ['admin-subscription-plans'] as const
export const adminUserSubscriptionsQueryKey = ['admin-user-subscriptions'] as const
export const currentUserSubscriptionsQueryKey = ['current-user-subscriptions'] as const
export const currentSubscriptionCatalogQueryKey = ['current-subscription-catalog'] as const
export const currentSubscriptionOrderQueryKey = ['current-subscription-order'] as const

/** 按计划主键倒序读取管理员计划目录。 */
export function useAdminSubscriptionPlans(pageSize = 25) {
  return useInfiniteQuery({
    queryKey: [...adminSubscriptionPlansQueryKey, { pageSize }],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminSubscriptionPlans({
        client: apiClient,
        query: { before: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 只在管理员选定用户后读取该用户的订阅事实。 */
export function useAdminUserSubscriptions(userId: number | undefined) {
  return useInfiniteQuery({
    queryKey: [...adminUserSubscriptionsQueryKey, userId],
    enabled: userId !== undefined,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (userId === undefined) throw new Error('订阅查询缺少用户 ID')
      const { data } = await listAdminUserSubscriptions({
        client: apiClient,
        path: { user_id: userId },
        query: { before: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 按当前会话主体读取本人订阅，不接受客户端所有权参数。 */
export function useCurrentUserSubscriptions() {
  return useInfiniteQuery({
    queryKey: currentUserSubscriptionsQueryKey,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listCurrentUserSubscriptions({
        client: apiClient,
        query: { before: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 读取当前会话可购买的计划与服务端筛选后的价格快照。 */
export function useCurrentSubscriptionCatalog() {
  return useQuery({
    queryKey: currentSubscriptionCatalogQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await listCurrentSubscriptionCatalog({
        client: apiClient,
        signal,
      })
      return data
    },
  })
}

/** 为当前用户创建或幂等恢复一笔待支付订阅订单。 */
export function useCreateCurrentSubscriptionOrder() {
  return useMutation({
    mutationFn: async (body: SubscriptionOrderCreateRequest) => {
      const { data } = await createCurrentSubscriptionOrder({ body, client: apiClient })
      return data
    },
  })
}

/** 读取当前用户可见的单笔订单，用于支付返回后的状态核对和原订单恢复。 */
export function useCurrentSubscriptionOrder(orderId: string | undefined) {
  return useQuery({
    queryKey: [...currentSubscriptionOrderQueryKey, orderId],
    enabled: orderId !== undefined,
    queryFn: async ({ signal }) => {
      if (orderId === undefined) throw new Error('缺少订阅订单 ID')
      const { data } = await getCurrentSubscriptionOrder({
        client: apiClient,
        path: { order_id: orderId },
        signal,
      })
      return data
    },
  })
}

/** 提交或恢复同一订阅订单的 Provider 支付会话，不在客户端推导支付结果。 */
export function useSubmitCurrentSubscriptionOrderPayment() {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ orderId, body }: { orderId: string; body: SubscriptionOrderPaymentRequest }) => {
      const { data } = await submitCurrentSubscriptionOrderPayment({
        body,
        client: apiClient,
        path: { order_id: orderId },
      })
      return data
    },
    onSuccess: (data) => {
      // 支付确认后重新读取订阅事实，界面不能从订单状态臆造已生效。
      if (data.order.status === 'paid') {
        void queryClient.invalidateQueries({ queryKey: currentUserSubscriptionsQueryKey })
      }
    },
  })
}

/** 创建成功后重新读取计划目录，避免前端伪造服务端版本。 */
export function useCreateAdminSubscriptionPlan() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminSubscriptionPlanCreateRequest) => {
      const { data } = await createAdminSubscriptionPlan({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminSubscriptionPlansQueryKey }),
  })
}

/** 以服务端返回的计划版本执行 CAS 停用。 */
export function useDisableAdminSubscriptionPlan() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({
      planId,
      body,
    }: {
      planId: string
      body: AdminSubscriptionPlanDisableRequest
    }) => {
      const { data } = await disableAdminSubscriptionPlan({
        body,
        client: apiClient,
        path: { plan_id: planId },
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminSubscriptionPlansQueryKey }),
  })
}

/** 给路径指定用户绑定计划，并校正管理员与本人订阅缓存。 */
export function useBindAdminUserSubscription() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({
      userId,
      body,
    }: {
      userId: number
      body: AdminUserSubscriptionBindRequest
    }) => {
      const { data } = await bindAdminUserSubscription({
        body,
        client: apiClient,
        path: { user_id: userId },
      })
      return data
    },
    onSuccess: (_data, variables) => Promise.all([
      queryClient.invalidateQueries({
        queryKey: [...adminUserSubscriptionsQueryKey, variables.userId],
      }),
      queryClient.invalidateQueries({ queryKey: currentUserSubscriptionsQueryKey }),
    ]),
  })
}

/** 使用订阅当前版本迁移生命周期，并校正管理员与本人订阅缓存。 */
export function useTransitionAdminUserSubscriptionLifecycle() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({
      userId,
      subscriptionId,
      body,
    }: {
      userId: number
      subscriptionId: string
      body: AdminUserSubscriptionLifecycleRequest
    }) => {
      const { data } = await transitionAdminUserSubscriptionLifecycle({
        body,
        client: apiClient,
        path: { user_id: userId, subscription_id: subscriptionId },
      })
      return data
    },
    onSuccess: (_data, variables) => Promise.all([
      queryClient.invalidateQueries({
        queryKey: [...adminUserSubscriptionsQueryKey, variables.userId],
      }),
      queryClient.invalidateQueries({ queryKey: currentUserSubscriptionsQueryKey }),
    ]),
  })
}

/** 提取订阅接口的稳定公开错误码。 */
export function subscriptionErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}
