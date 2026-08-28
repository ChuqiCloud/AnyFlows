import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  changeOrganizationServiceAccountStatus,
  createOrganizationServiceAccount,
  getOrganizationServiceAccountBudgetBinding,
  listOrganizationServiceAccounts,
  rotateOrganizationServiceAccountKey,
  setOrganizationServiceAccountBudgetBinding,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationServiceAccountBudgetBindingRequest,
  OrganizationServiceAccountCreateRequest,
  OrganizationServiceAccountRotateRequest,
  OrganizationServiceAccountStatusRequest,
} from '@/lib/api/generated/types.gen'

const serviceAccountQueryRoot = ['organization-service-accounts'] as const

function serviceAccountQueryKey(organizationId: number, after: string | undefined, limit: number) {
  return [...serviceAccountQueryRoot, organizationId, after ?? null, limit] as const
}

function serviceAccountBudgetBindingQueryKey(organizationId: number, serviceAccountKey: string) {
  return [...serviceAccountQueryRoot, organizationId, 'budget-binding', serviceAccountKey] as const
}

/** 读取企业范围的服务账号目录；游标始终绑定当前企业。 */
export function useOrganizationServiceAccounts(
  organizationId: number,
  after: string | undefined,
  limit: number,
  enabled: boolean,
) {
  return useQuery({
    queryKey: serviceAccountQueryKey(organizationId, after, limit),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationServiceAccounts({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { after, limit },
        signal,
      })
      return data
    },
  })
}

export function useCreateOrganizationServiceAccount(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async (body: OrganizationServiceAccountCreateRequest) => {
      const { data } = await createOrganizationServiceAccount({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateServiceAccounts(queryClient, organizationId),
  })
}

export function useChangeOrganizationServiceAccountStatus(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ serviceAccountKey, body }: { serviceAccountKey: string; body: OrganizationServiceAccountStatusRequest }) => {
      const { data } = await changeOrganizationServiceAccountStatus({
        body,
        client: apiClient,
        path: { organization_id: organizationId, service_account_key: serviceAccountKey },
      })
      return data
    },
    onSuccess: () => invalidateServiceAccounts(queryClient, organizationId),
  })
}

export function useRotateOrganizationServiceAccountKey(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ serviceAccountKey, body }: { serviceAccountKey: string; body: OrganizationServiceAccountRotateRequest }) => {
      const { data } = await rotateOrganizationServiceAccountKey({
        body,
        client: apiClient,
        path: { organization_id: organizationId, service_account_key: serviceAccountKey },
      })
      return data
    },
    onSuccess: () => invalidateServiceAccounts(queryClient, organizationId),
  })
}

/** 读取当前绑定；服务端以 404 表示尚未绑定，前端将其呈现为可创建状态。 */
export function useOrganizationServiceAccountBudgetBinding(
  organizationId: number,
  serviceAccountKey: string | undefined,
  enabled: boolean,
) {
  return useQuery({
    queryKey: serviceAccountBudgetBindingQueryKey(organizationId, serviceAccountKey ?? 'none'),
    enabled: enabled && serviceAccountKey !== undefined,
    queryFn: async ({ signal }) => {
      try {
        const { data } = await getOrganizationServiceAccountBudgetBinding({
          client: apiClient,
          path: { organization_id: organizationId, service_account_key: serviceAccountKey! },
          signal,
        })
        return data
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) return null
        throw error
      }
    },
  })
}

export function useSetOrganizationServiceAccountBudgetBinding(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ serviceAccountKey, body }: { serviceAccountKey: string; body: OrganizationServiceAccountBudgetBindingRequest }) => {
      const { data } = await setOrganizationServiceAccountBudgetBinding({
        body,
        client: apiClient,
        path: { organization_id: organizationId, service_account_key: serviceAccountKey },
      })
      return data
    },
    onSuccess: (_, variables) => {
      void queryClient.invalidateQueries({
        queryKey: serviceAccountBudgetBindingQueryKey(organizationId, variables.serviceAccountKey),
      })
    },
  })
}

export function organizationServiceAccountErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}

export function organizationServiceAccountErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}

function invalidateServiceAccounts(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return queryClient.invalidateQueries({ queryKey: [...serviceAccountQueryRoot, organizationId] })
}
